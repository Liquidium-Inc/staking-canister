use base64::{self, prelude::BASE64_STANDARD, Engine};
use bitcoin::{psbt::Psbt, Address, AddressType, Network};
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

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct ValidationDetails {
    /// Primary pool address
    pub pool_address: String,
    /// Secondary pool address
    pub secondary_pool_address: String,
    /// User address from transaction
    pub user_address: Option<String>,
    /// Rune totals by address and type
    pub rune_totals: RuneTotals,
    /// Transaction exchange ratio
    pub unstake_exchange_ratio: f64,
    /// API exchange ratio
    pub api_exchange_ratio: Option<f64>,
    /// Result message
    pub validation_result: String,
    /// Secondary pool UTXO
    pub secondary_pool_utxo: Option<String>,
    /// All processed rune edicts for debugging
    pub rune_edicts: Vec<EdictDetails>,
    /// LIQ utxos to pool
    pub canister_liq_utxos: Vec<u64>,
}

// Rune totals for each address and type
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct RuneTotals {
    /// LIQ rune ID
    pub liq_id: String,
    /// sLIQ rune ID
    pub sliq_id: String,
    /// LIQ to primary pool
    pub pool_liq_value: u128,
    /// sLIQ to primary pool
    pub pool_sliq_value: u128,
    /// LIQ to secondary pool
    pub secondary_pool_liq_value: u128,
    /// sLIQ to secondary pool
    pub secondary_pool_sliq_value: u128,
    /// LIQ to user
    pub user_liq_value: u128,
    /// sLIQ to user
    pub user_sliq_value: u128,
}

#[derive(Debug, Error)]
pub enum UnstakeValidationError {
    #[error("Invalid PSBT: {0}")]
    InvalidPsbt(String),
    #[error("Transaction fee is too low")]
    FeeTooLow,
    #[error("Transaction fee is too high")]
    FeeTooHigh,
    #[error("{0}")]
    Other(String),
}

// Main entry for unstake PSBT validation
pub async fn validate_unstake(
    psbt_bytes: &[u8],
) -> Result<(bool, ValidationDetails), UnstakeValidationError> {
    let mut details = ValidationDetails::default();

    let primary_pool_address = state::get_pool_address(0);
    details.pool_address = primary_pool_address.clone();

    if primary_pool_address.is_empty() {
        return Err(UnstakeValidationError::Other(
            "Primary pool address not initialized".to_string(),
        ));
    }

    let secondary_pool_address = state::get_pool_address(1);
    details.secondary_pool_address = secondary_pool_address.clone();

    if secondary_pool_address.is_empty() {
        return Err(UnstakeValidationError::Other(
            "Secondary pool address not initialized".to_string(),
        ));
    }

    let psbt = psbt::deserialize_psbt(psbt_bytes)
        .map_err(|e| UnstakeValidationError::InvalidPsbt(e.to_string()))?;

    let psbt_base64 = BASE64_STANDARD.encode(psbt_bytes);

    let txid = psbt.clone().extract_tx().txid().to_string();

    // Validate rune exchange ratio and collect details
    let (is_valid, mut validation_details) = validate_rune_exchange_ratio(
        &psbt_base64,
        &txid,
        &primary_pool_address,
        &secondary_pool_address,
        &psbt,
    )
    .await?;

    validate_psbt_structure(&psbt, &mut validation_details)?;
    validate_fees(&psbt)?;

    validation_details.pool_address = primary_pool_address;
    validation_details.secondary_pool_address = secondary_pool_address;
    validation_details.validation_result = "Success".to_string();

    Ok((is_valid, validation_details))
}

// Checks PSBT structure and required UTXO
fn validate_psbt_structure(
    psbt: &Psbt,
    validation_details: &mut ValidationDetails,
) -> Result<(), UnstakeValidationError> {
    if psbt.inputs.is_empty() {
        return Err(UnstakeValidationError::InvalidPsbt(
            "PSBT has no inputs".to_string(),
        ));
    }
    if psbt.unsigned_tx.output.is_empty() {
        return Err(UnstakeValidationError::InvalidPsbt(
            "PSBT has no outputs".to_string(),
        ));
    }
    if validation_details.secondary_pool_utxo.is_none() {
        return Err(UnstakeValidationError::Other(
            "No UTXO found in secondary pool".to_string(),
        ));
    }

    if psbt.unsigned_tx.output.len() != 7 && psbt.unsigned_tx.output.len() != 6 {
        return Err(UnstakeValidationError::InvalidPsbt(
            "PSBT is malformed".to_string(),
        ));
    }

    if psbt.unsigned_tx.output[0].value != 0 {
        return Err(UnstakeValidationError::InvalidPsbt(
            "PSBT is malformed missing OP_RETURN".to_string(),
        ));
    }

    let btc_network = state::get_btc_network();
    let pool_address = state::get_pool_address(0);
    let secondary_pool_address = state::get_pool_address(1);
    // Parse outputs in reverse
    let mut index = psbt
        .unsigned_tx
        .output
        .len()
        .checked_sub(1)
        .expect("bug: underflow");

    let (address, addr_type) = decode_output_address(psbt, btc_network, index)
        .map_err(UnstakeValidationError::InvalidPsbt)?;

    // We have change, move index and validate address type
    if address != secondary_pool_address {
        if addr_type != AddressType::P2wpkh
            && addr_type != AddressType::P2tr
            && addr_type != AddressType::P2sh
        {
            return Err(UnstakeValidationError::InvalidPsbt(
                "Invalid address type".to_string(),
            ));
        }

        index = index.checked_sub(1).expect("bug: underflow");
    }

    let (address, _) = decode_output_address(psbt, btc_network, index)
        .map_err(UnstakeValidationError::InvalidPsbt)?;

    // Next we expect secondary pool address
    if address != secondary_pool_address {
        return Err(UnstakeValidationError::InvalidPsbt(format!(
            "Invalid secondary pool address {} != {}",
            address, secondary_pool_address,
        )));
    }

    // Move index
    index = index.checked_sub(1).expect("bug: underflow");
    let (address, addr_type) = decode_output_address(psbt, btc_network, index)
        .map_err(UnstakeValidationError::InvalidPsbt)?;

    // Next we expect user address should be tr or segwit
    if addr_type != AddressType::P2wpkh && addr_type != AddressType::P2tr {
        return Err(UnstakeValidationError::InvalidPsbt(
            "Invalid address type".to_string(),
        ));
    }

    if address == secondary_pool_address || address == pool_address {
        return Err(UnstakeValidationError::InvalidPsbt(
            "Unexpected address".to_string(),
        ));
    }

    // Move index
    index = index.checked_sub(1).expect("bug: underflow");

    // Next we expect primary pool address  sLIQ change
    let (address, _) = decode_output_address(psbt, btc_network, index)
        .map_err(UnstakeValidationError::InvalidPsbt)?;

    if address != pool_address {
        return Err(UnstakeValidationError::InvalidPsbt(format!(
            "Invalid pool address {} != {}",
            address, pool_address,
        )));
    }

    // Move index
    index = index.checked_sub(1).expect("bug: underflow");

    validation_details.canister_liq_utxos = vec![];
    // Next outputs should also be the the pool address
    while index > 0 {
        let (current_address, _) = decode_output_address(psbt, btc_network, index)
            .map_err(UnstakeValidationError::InvalidPsbt)?;

        if current_address != pool_address {
            return Err(UnstakeValidationError::InvalidPsbt(
                "Unexpected address".to_string(),
            ));
        }

        validation_details.canister_liq_utxos.push(index as u64);
        index = index.checked_sub(1).expect("bug: underflow");
    }

    if index != 0 {
        return Err(UnstakeValidationError::InvalidPsbt(
            "PSBT is malformed to many outputs".to_string(),
        ));
    }

    Ok(())
}

// Checks transaction fee bounds
fn validate_fees(psbt: &Psbt) -> Result<(), UnstakeValidationError> {
    let mut total_input = 0;
    for input in &psbt.inputs {
        if let Some(utxo) = &input.witness_utxo {
            total_input += utxo.value;
        }
    }
    let total_output: u64 = psbt
        .unsigned_tx
        .output
        .iter()
        .map(|output| output.value)
        .sum();
    if total_input <= total_output {
        return Err(UnstakeValidationError::Other(
            "Transaction has no fee".to_string(),
        ));
    }
    let fee = total_input - total_output;
    if fee < 500 {
        return Err(UnstakeValidationError::FeeTooLow);
    }
    if fee > 100000 {
        return Err(UnstakeValidationError::FeeTooHigh);
    }
    Ok(())
}

// Validates rune exchange ratio for unstake
async fn validate_rune_exchange_ratio(
    psbt_base64: &str,
    txid: &str,
    primary_pool_address: &str,
    secondary_pool_address: &str,
    psbt: &Psbt,
) -> Result<(bool, ValidationDetails), UnstakeValidationError> {
    let mut details = ValidationDetails::default();
    let mut is_valid = true;

    let (rune_data, _debug_output) =
        ordinals_runes::parse_psbt_runes_with_debug_legacy(psbt_base64)
            .map_err(|e| UnstakeValidationError::Other(format!("Failed to parse runes: {}", e)))?;

    if rune_data.edicts.is_empty() {
        return Err(UnstakeValidationError::Other(
            "No rune edicts found in PSBT".to_string(),
        ));
    }

    details.pool_address = primary_pool_address.to_string();
    details.secondary_pool_address = secondary_pool_address.to_string();

    let app_config = config::default_config();
    let liq_rune_id = app_config.bitcoin.liq_rune_id.clone();
    let sliq_rune_id = app_config.bitcoin.sliq_rune_id.clone();

    let mut rune_totals = RuneTotals {
        liq_id: liq_rune_id.clone(),
        sliq_id: sliq_rune_id.clone(),
        ..Default::default()
    };

    let mut user_address: Option<String> = psbt
        .unsigned_tx
        .output
        .iter()
        .filter_map(|item| Address::from_script(&item.script_pubkey, Network::Bitcoin).ok())
        .map(|addr| addr.to_string())
        .find(|addr| {
            // Find the taproot address that is not the pool or secondary pool
            addr != primary_pool_address
                && addr != secondary_pool_address
                && addr.starts_with("bc1p")
        });

    for edict in &rune_data.edicts {
        assert!(edict.amount > 0, "bug: edict should never be 0");
        if (edict.output as usize) >= psbt.outputs.len() {
            return Err(UnstakeValidationError::Other("bug: invalid tx".to_string()));
        }
        if let Some(address) = &edict.address {
            if address == primary_pool_address {
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
            } else if address == secondary_pool_address {
                if edict.id == liq_rune_id {
                    rune_totals.secondary_pool_liq_value = rune_totals
                        .secondary_pool_liq_value
                        .checked_add(edict.amount)
                        .expect("bug: overflow");
                } else if edict.id == sliq_rune_id {
                    rune_totals.secondary_pool_sliq_value = rune_totals
                        .secondary_pool_sliq_value
                        .checked_add(edict.amount)
                        .expect("bug: overflow");
                }
                let output_index = edict.output;
                let secondary_pool_utxo = format!("{}:{}", txid, output_index);
                details.secondary_pool_utxo = Some(secondary_pool_utxo);
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

    // Must receive sLIQ in primary pool and LIQ in secondary pool
    if details.rune_totals.pool_sliq_value == 0 {
        return Err(UnstakeValidationError::Other(
            "No sLIQ runes sent to primary pool address".to_string(),
        ));
    }
    if details.rune_totals.secondary_pool_liq_value == 0 {
        return Err(UnstakeValidationError::Other(
            "No LIQ runes sent to secondary pool address".to_string(),
        ));
    }

    let unstake_exchange_ratio = if details.rune_totals.secondary_pool_liq_value > 0 {
        details.rune_totals.secondary_pool_liq_value as f64
            / details.rune_totals.pool_sliq_value as f64
    } else {
        1.0
    };
    details.unstake_exchange_ratio = unstake_exchange_ratio;

    let exchange_rate_result = match crate::state::get_stored_exchange_rate() {
        Some(stored_rate) => Ok(stored_rate),
        None => Err("No exchange rate available in storage".to_string()),
    };

    match exchange_rate_result {
        Ok(exchange_rate) => {
            details.api_exchange_ratio = Some(exchange_rate);
            if exchange_rate > 0.0 {
                let ratio = unstake_exchange_ratio / exchange_rate;
                let ratio_delta = 1.0 - ratio;

                // We need to make sure that exchange rate delta is:
                //     - grather than 0 : ensures that the exchange rate is in the favor of the protocol
                //     - less then 0.01 : mitigates ratio manipulation attempts
                if ratio_delta >= 0.0 && ratio_delta < 0.01 {
                    details.validation_result = format!(
                        "Rune exchange ratio validation passed: transaction ratio ({}) is favorable to protocol (stored rate: {})",
                        unstake_exchange_ratio, exchange_rate
                    );
                    is_valid = true;
                } else {
                    details.validation_result = format!(
                        "Warning: Unstake exchange ratio ({}) is higher than stored rate ({}), which is unfavorable to protocol",
                        unstake_exchange_ratio, exchange_rate
                    );
                    is_valid = false;
                }
            } else {
                details.validation_result =
                    "Zero exchange rate returned, skipping ratio validation".to_string();
            }
        }
        Err(e) => {
            details.validation_result = format!("Exchange rate unavailable: {}", e);
        }
    }

    validate_values(psbt, &mut details, &mut is_valid).await;

    Ok((is_valid, details))
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

    // Calculate the total amount of sliq runes that are sent
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

    // Make sure that the user sends the sLIQ tokens to the pool
    if rune_amount < details.rune_totals.pool_sliq_value {
        details.validation_result = "Insufficient stake rune amounts".to_string();
        *is_valid = false;
    }

    // Make sure that the user is receiving his sliq change
    let expected_sliq_change = rune_amount - details.rune_totals.pool_sliq_value;
    if expected_sliq_change != details.rune_totals.user_sliq_value {
        details.validation_result = "Invalid sLIQ change".to_string();
        *is_valid = false;
    }

    // Make sure that the POOL is receiving the correct liq change
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

    // Make sure that the POOL is getting the liq change
    let expected_liq_change = rune_amount
        .checked_sub(details.rune_totals.secondary_pool_liq_value)
        .expect("bug: subtration failed");

    if expected_liq_change != details.rune_totals.pool_liq_value {
        details.validation_result = "Invalid LIQ change".to_string();
        *is_valid = false;
    }
}
