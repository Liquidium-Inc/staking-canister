use base64::{self, prelude::BASE64_STANDARD, Engine};
use bitcoin::{psbt::Psbt, AddressType};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    bitcoin::psbt,
    config::{self, default_config},
    oracle::ord_client::OrdClientTrait,
    state::{self, get_ord_client},
    validation::utils::decode_output_address,
};

/// Rune edict details for debugging
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EdictDetails {
    /// Rune ID
    pub id: String,
    /// Amount transferred
    pub amount: u128,
    /// Destination address
    pub address: Option<String>,
    /// Output index
    pub output: u32,
}

/// Validation details container
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ValidationDetails {
    /// Pool address
    pub pool_address: String,
    /// User address from transaction
    pub user_address: Option<String>,
    /// Rune totals by address and type
    pub rune_totals: RuneTotals,
    /// Transaction exchange ratio
    pub stake_exchange_ratio: f64,
    /// API exchange ratio
    pub api_exchange_ratio: Option<f64>,
    /// Result message
    pub validation_result: String,
    /// All processed rune edicts for debugging
    pub rune_edicts: Vec<EdictDetails>,
    // The utxos that the canister needs to sign
    pub utxos_to_sign: Vec<usize>,
    // Canister liq utxos
    pub canister_liq_utxos: Vec<u64>,
}

/// Rune totals by address and type
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct RuneTotals {
    /// LIQ rune ID
    pub liq_id: String,
    /// sLIQ rune ID
    pub sliq_id: String,
    /// LIQ to pool
    pub pool_liq_value: u128,
    /// sLIQ to pool
    pub pool_sliq_value: u128,
    /// LIQ to user
    pub user_liq_value: u128,
    /// sLIQ to user
    pub user_sliq_value: u128,
}

/// Stake validation errors
#[derive(Debug, Error)]
pub enum StakeValidationError {
    #[error("Invalid PSBT: {0}")]
    InvalidPsbt(String),

    #[error("Missing output to pool address")]
    MissingPoolOutput,

    #[error("Insufficient funds in inputs: total_input={0}, total_output={1}")]
    InsufficientFunds(u64, u64),

    #[error("Transaction fee is too low: {0} sats (minimum 1000 sats)")]
    FeeTooLow(u64),

    #[error("Transaction fee is too high: {0} sats (maximum 100000 sats)")]
    FeeTooHigh(u64),

    #[error("{0}")]
    Other(String),
}

/// Validates stake PSBT and returns result with details
///
/// Returns (success, details) tuple
pub async fn validate_stake(
    psbt_base64: &str,
) -> Result<(bool, ValidationDetails), StakeValidationError> {
    // Validate psbt format
    if psbt_base64.len() > 100_000 {
        return Err(StakeValidationError::InvalidPsbt(
            "Psbt is to large".to_string(),
        ));
    }

    // Decode base64 input
    let psbt_bytes = BASE64_STANDARD.decode(psbt_base64).map_err(|e| {
        StakeValidationError::InvalidPsbt(format!("Failed to decode base64: {}", e))
    })?;

    // Deserialize PSBT
    let psbt = psbt::deserialize_psbt(&psbt_bytes)
        .map_err(|e| StakeValidationError::InvalidPsbt(e.to_string()))?;

    let mut validation_details = ValidationDetails::default();

    // Run validations - these return () on success, so we just check for errors
    validate_psbt_structure(&psbt, &mut validation_details)?;
    validate_outputs(&psbt)?;
    validate_fees(&psbt)?;

    // Validate rune exchange ratio and collect details
    let is_valid =
        validate_rune_exchange_ratio(psbt_base64, &psbt, &mut validation_details).await?;

    // Only set success message if validation passed
    if is_valid {
        validation_details.validation_result = "Success".to_string();
    }

    // Return validation result, along with the details
    Ok((is_valid, validation_details))
}

/// Checks PSBT structure validity
fn validate_psbt_structure(
    psbt: &Psbt,
    validation_details: &mut ValidationDetails,
) -> Result<(), StakeValidationError> {
    // Check if the PSBT has inputs
    if psbt.inputs.is_empty() {
        return Err(StakeValidationError::InvalidPsbt(
            "PSBT has no inputs".to_string(),
        ));
    }

    if psbt.unsigned_tx.output.len() > 7 {
        return Err(StakeValidationError::InvalidPsbt(
            "PSBT is malformed".to_string(),
        ));
    }

    if psbt.unsigned_tx.output[0].value != 0 {
        return Err(StakeValidationError::InvalidPsbt(
            "PSBT is malformed missing OP_RETURN".to_string(),
        ));
    }

    // Parse outputs in reverse
    let mut index = psbt
        .unsigned_tx
        .output
        .len()
        .checked_sub(1)
        .expect("bug: underflow");

    let btc_network = state::get_btc_network();
    let pool_address = state::get_pool_address(0);

    // Last ouptut should be the user address
    let (mut current_address, addr_type) = decode_output_address(psbt, btc_network, index)
        .map_err(StakeValidationError::InvalidPsbt)?;

    if !is_allowed_type(&addr_type) {
        return Err(StakeValidationError::InvalidPsbt(format!(
            "Unsupported address type {}",
            current_address
        )));
    }

    let (next_address, _) = decode_output_address(
        psbt,
        btc_network,
        index.checked_sub(1).expect("bug: underflow"),
    )
    .map_err(StakeValidationError::InvalidPsbt)?;

    if next_address != current_address {
        index = index.checked_sub(1).expect("bug: underflow");
    }

    index = index.checked_sub(1).expect("bug: underflow");
    current_address = next_address;
    // Next outputs should also be the user address until we find
    // the pool address, after which all addresses should be the pool address
    while index > 0 {
        let (next_address, addr_type) = decode_output_address(psbt, btc_network, index)
            .map_err(StakeValidationError::InvalidPsbt)?;

        if addr_type != AddressType::P2tr && addr_type != AddressType::P2wpkh {
            return Err(StakeValidationError::InvalidPsbt(
                "Unsupported address type".to_string(),
            ));
        }

        if current_address != next_address && next_address != pool_address {
            return Err(StakeValidationError::InvalidPsbt(
                "Unexpected address".to_string(),
            ));
        }

        current_address = next_address;
        if current_address == pool_address && validation_details.canister_liq_utxos.is_empty() {
            validation_details.canister_liq_utxos = vec![index as u64];
        }

        index = index.checked_sub(1).expect("bug: underflow");
    }

    if index != 0 {
        return Err(StakeValidationError::InvalidPsbt(
            "PSBT is malformed to many outputs".to_string(),
        ));
    }

    Ok(())
}

/// Verifies PSBT outputs
fn validate_outputs(psbt: &Psbt) -> Result<(), StakeValidationError> {
    // Use primary pool address (index 0)
    let pool_address = state::get_pool_address(0);
    if pool_address.is_empty() {
        return Err(StakeValidationError::Other(
            "Pool address not initialized".to_string(),
        ));
    }

    // RJJ-TODO | Check for non-empty script outputs
    let has_output = psbt
        .unsigned_tx
        .output
        .iter()
        .any(|output| !output.script_pubkey.is_empty());

    if !has_output {
        return Err(StakeValidationError::MissingPoolOutput);
    }

    Ok(())
}

fn is_allowed_type(addr_type: &AddressType) -> bool {
    matches!(
        addr_type,
        AddressType::P2wpkh | AddressType::P2sh | AddressType::P2tr
    )
}

/// Checks transaction fee bounds
fn validate_fees(psbt: &Psbt) -> Result<(), StakeValidationError> {
    // Calculate total input value
    let mut total_input = 0;
    for input in &psbt.inputs {
        if let Some(utxo) = &input.witness_utxo {
            total_input += utxo.value;
        }
    }

    // Calculate total output value
    let total_output: u64 = psbt
        .unsigned_tx
        .output
        .iter()
        .map(|output| output.value)
        .sum();

    // Calculate fee
    if total_input <= total_output {
        return Err(StakeValidationError::InsufficientFunds(
            total_input,
            total_output,
        ));
    }
    let fee = total_input - total_output;

    // Check for minimum fee (500 sats)
    if fee < 500 {
        return Err(StakeValidationError::FeeTooLow(fee));
    }

    // Check for maximum fee (100000 sats)
    if fee > 100000 {
        return Err(StakeValidationError::FeeTooHigh(fee));
    }

    Ok(())
}

pub async fn validate_rune_exchange_ratio(
    psbt_base64: &str,
    psbt: &Psbt,
    details: &mut ValidationDetails,
) -> Result<bool, StakeValidationError> {
    // let mut details = ValidationDetails::default();
    let mut is_valid = true;

    let (rune_data, _debug_output) =
        ordinals_runes::parse_psbt_runes_with_debug_legacy(psbt_base64)
            .map_err(|e| StakeValidationError::Other(format!("Failed to parse runes: {}", e)))?;

    if rune_data.edicts.is_empty() {
        return Err(StakeValidationError::Other(
            "No rune edicts found in PSBT".to_string(),
        ));
    }

    let pool_address = state::get_pool_address(0);
    details.pool_address = pool_address.clone();

    if pool_address.is_empty() {
        return Err(StakeValidationError::Other(
            "Pool address not initialized".to_string(),
        ));
    }

    let app_config = config::default_config();
    let liq_rune_id = app_config.bitcoin.liq_rune_id.clone();
    let sliq_rune_id = app_config.bitcoin.sliq_rune_id.clone();

    let mut rune_totals = RuneTotals {
        liq_id: liq_rune_id.clone(),
        sliq_id: sliq_rune_id.clone(),
        ..Default::default()
    };

    let mut user_address: Option<String> = None;
    let mut rune_edicts = Vec::new();

    // Process all edicts and capture details for debugging
    for edict in &rune_data.edicts {
        assert!(edict.amount > 0, "bug: edict should never be 0");

        if (edict.output as usize) >= psbt.outputs.len() {
            return Err(StakeValidationError::Other("bug: invalid tx".to_string()));
        }

        // Capture edict details for response
        let edict_detail = EdictDetails {
            id: edict.id.clone(),
            amount: edict.amount,
            address: edict.address.clone(),
            output: edict.output,
        };

        rune_edicts.push(edict_detail);

        if let Some(address) = &edict.address {
            if address == &pool_address {
                if edict.id == liq_rune_id {
                    rune_totals.pool_liq_value = rune_totals
                        .pool_liq_value
                        .checked_add(edict.amount)
                        .expect("bug: overflow");
                } else if edict.id == sliq_rune_id {
                    rune_totals.pool_sliq_value = rune_totals
                        .pool_sliq_value
                        .checked_add(edict.amount)
                        .expect("bug: overflow");
                }
            } else {
                if user_address.is_none() {
                    user_address = Some(address.clone());
                }
                if edict.id == liq_rune_id {
                    rune_totals.user_liq_value = rune_totals
                        .user_liq_value
                        .checked_add(edict.amount)
                        .expect("bug: overflow");
                } else if edict.id == sliq_rune_id {
                    rune_totals.user_sliq_value = rune_totals
                        .user_sliq_value
                        .checked_add(edict.amount)
                        .expect("bug: overflow");
                }
            }
        }
    }

    details.rune_totals = rune_totals;
    details.user_address = user_address;
    details.rune_edicts = rune_edicts;

    if details.rune_totals.pool_liq_value == 0 {
        return Err(StakeValidationError::Other(
            "No LIQ runes sent to pool address".to_string(),
        ));
    }

    if details.rune_totals.user_sliq_value == 0 {
        return Err(StakeValidationError::Other(
            "No sLIQ runes returned to user wallet".to_string(),
        ));
    }

    let stake_exchange_ratio =
        details.rune_totals.pool_liq_value as f64 / details.rune_totals.user_sliq_value as f64;
    details.stake_exchange_ratio = stake_exchange_ratio;

    let exchange_rate = crate::state::get_stored_exchange_rate().ok_or_else(|| {
        StakeValidationError::Other("No exchange rate available in storage".to_string())
    })?;

    if exchange_rate <= 0.0 {
        return Err(StakeValidationError::Other(format!(
            "Invalid stored exchange rate: {}",
            exchange_rate
        )));
    }

    details.api_exchange_ratio = Some(exchange_rate);

    let ratio = stake_exchange_ratio / exchange_rate;
    let ratio_delta = ratio - 1.0;

    // We need to make sure that exchange rate delta is:
    //     - grather than 0 : ensures that the exchange rate is in the favor of the protocol
    //     - less then 0.01 : mitigates ratio manipulation attempts

    if ratio_delta >= 0.0 && ratio_delta < 0.01 {
        details.validation_result = format!(
            "Rune exchange ratio validation passed: transaction ratio ({}) is favorable to protocol (stored rate: {})",
            stake_exchange_ratio, exchange_rate
        );
        is_valid = true;
    } else {
        details.validation_result = format!(
            "Warning: Stake exchange ratio ({}) is lower than stored rate ({}), which is unfavorable to protocol",
            stake_exchange_ratio, exchange_rate
        );
        is_valid = false;
    }

    // Ensure that the rune amounts are as expected
    validate_values(psbt, details, &mut is_valid).await;

    Ok(is_valid)
}

async fn validate_values(
    psbt: &bitcoin::psbt::PartiallySignedTransaction,
    details: &mut ValidationDetails,
    is_valid: &mut bool,
) {
    // Check that the runes are actually sent
    let utxos = psbt
        .unsigned_tx
        .input
        .iter()
        .map(|item| item.previous_output.to_string())
        .collect::<Vec<String>>();

    let ord_client = get_ord_client();

    // Fetch the utxo info map
    let utxo_info_map = ord_client
        .get_rune_utxo_info_map(&utxos)
        .await
        .expect("Could not fetch utxo info");

    // Calculate the total amount of liq runes that are sent
    let rune_id = default_config().bitcoin.liq_rune_id;
    let rune_amount: u128 = utxo_info_map
        .values()
        .flatten() // skip None
        .filter_map(|info| {
            // Pair ids with balances and find the matching one
            info.rune_ids
                .iter()
                .zip(info.rune_balances.iter())
                .find(|(id, _)| *id == &rune_id)
                .map(|(_, bal)| *bal)
        })
        .sum();

    // Make sure that the user sends the LIQ tokens to the pool
    if rune_amount < details.rune_totals.pool_liq_value {
        details.validation_result = "Insufficient stake rune amounts".to_string();
        *is_valid = false;
    }

    // Make sure that the user is receiving his liq change
    let expected_liq_change = rune_amount
        .checked_sub(details.rune_totals.pool_liq_value)
        .expect("bug: underflow");
    if expected_liq_change != details.rune_totals.user_liq_value {
        details.validation_result = "Invalid LIQ change".to_string();
        *is_valid = false;
    }

    // Make sure that the pool is receiving the correct sLIQ change
    let rune_id = default_config().bitcoin.sliq_rune_id;
    let rune_amount: u128 = utxo_info_map
        .values()
        .flatten() // skip None
        .filter_map(|info| {
            // Pair ids with balances and find the matching one
            info.rune_ids
                .iter()
                .zip(info.rune_balances.iter())
                .find(|(id, _)| *id == &rune_id)
                .map(|(_, bal)| *bal)
        })
        .sum();

    // Make sure that the POOL is getting the sLiq change
    let expected_sliq_change = rune_amount
        .checked_sub(details.rune_totals.user_sliq_value)
        .expect("bug: substraction failed");

    if expected_sliq_change != details.rune_totals.pool_sliq_value {
        details.validation_result = "Invalid sLIQ change".to_string();
        *is_valid = false;
    }

    // We only sign the sliq inputs from the validation details
    let sliq_utxos = utxo_info_map
        .iter()
        .filter_map(|(outpoint, info_opt)| {
            info_opt.as_ref().and_then(|info| {
                if info.rune_ids.iter().any(|id| *id == rune_id) {
                    Some(outpoint.clone()) // outpoint is the key
                } else {
                    None
                }
            })
        })
        .collect::<Vec<String>>();

    let sliq_utxos: Vec<usize> = psbt
        .unsigned_tx
        .input
        .iter()
        .enumerate()
        .filter_map(|item| {
            if sliq_utxos.contains(&item.1.previous_output.to_string()) {
                Some(item.0)
            } else {
                None
            }
        })
        .collect();

    if sliq_utxos.is_empty() {
        details.validation_result = "No SLIQ inputs to sign".to_string();
        *is_valid = false;
    }

    details.utxos_to_sign = sliq_utxos;
}
