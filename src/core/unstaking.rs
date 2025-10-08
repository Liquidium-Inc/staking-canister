use crate::bitcoin::psbt::deserialize_psbt;
use crate::config::default_config;
use crate::log::LOG;
use crate::oracle::omnity::OmnityOrdClient;
use crate::oracle::ord_client::OrdClientTrait;
use crate::state::transactions_storage::{self, TxRecord, TxTypeEnum, UnstakeUtxo};
use crate::state::{apply_rewards, get_ord_client};
use crate::types::{CanisterResponse, DebugInfo};
use crate::{
    bitcoin::psbt::{self, PsbtError},
    core::unstake_helper::{self, UnstakeHelperError},
    state,
    validation::unstake::{validate_unstake, UnstakeValidationError},
};
use base64::prelude::BASE64_STANDARD;
use base64::{self, Engine};
use bitcoin::psbt::Psbt;
use ic_canister_log::log;
use ic_cdk::api::time;
use serde_json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UnstakingError {
    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Signing error: {0}")]
    SigningError(String),

    #[error("{0}")]
    Other(String),
}

impl From<UnstakeValidationError> for UnstakingError {
    fn from(err: UnstakeValidationError) -> Self {
        UnstakingError::ValidationError(err.to_string())
    }
}

impl From<PsbtError> for UnstakingError {
    fn from(err: PsbtError) -> Self {
        UnstakingError::SigningError(err.to_string())
    }
}

impl From<UnstakeHelperError> for UnstakingError {
    fn from(err: UnstakeHelperError) -> Self {
        UnstakingError::Other(err.to_string())
    }
}

/// Validates and signs unstake PSBT
pub async fn unstake(input_psbt_base64: String) -> Result<String, UnstakingError> {
    // Apply rewards before staking
    apply_rewards();

    // Validate psbt format
    if input_psbt_base64.len() > 100_000 {
        return Err(UnstakingError::ValidationError(
            "Psbt is to large".to_string(),
        ));
    }
    // Decode base64 input
    let decoded = BASE64_STANDARD
        .decode(&input_psbt_base64)
        .map_err(|e| UnstakingError::Other(format!("Failed to decode base64 input: {}", e)))?;

    // Make sure that omnity is in sync
    OmnityOrdClient::check_sync(true)
        .await
        .map_err(|e| UnstakingError::Other(format!("Out of sync: {}", e)))?;

    // We should only sign liq inputss
    let inputs = get_inputs_to_sign(&decoded).await?;
    let inputs_to_sign = inputs.iter().map(|input| input.0).collect();

    // Sign PSBT with key_index 0 (primary pool)
    // We do the signing first so that we get the true txid
    let signed_psbt = match psbt::sign(&decoded, Some(0), Some(inputs_to_sign)).await {
        Ok(signed) => signed,
        Err(err) => {
            let response = CanisterResponse {
                signed_psbt: String::new(),
                error: Some(format!("Signing error: {}", err)),
                debug: DebugInfo {},
            };

            let json_response = serde_json::to_string(&response).map_err(|e| {
                UnstakingError::Other(format!("Failed to serialize response: {}", e))
            })?;

            return Ok(json_response);
        }
    };

    // Validate PSBT
    let (is_valid, validation_details) = match validate_unstake(&signed_psbt).await {
        Ok((is_valid, validation_details)) => (is_valid, validation_details),
        Err(err) => {
            let response = CanisterResponse {
                signed_psbt: String::new(),
                error: Some(err.to_string()),
                debug: DebugInfo {},
            };
            let json_response = serde_json::to_string(&response).map_err(|e| {
                UnstakingError::Other(format!("Failed to serialize response: {}", e))
            })?;
            return Ok(json_response);
        }
    };

    if !is_valid {
        let response = CanisterResponse {
            signed_psbt: String::new(),
            error: Some("Validation failed".to_string()),
            debug: DebugInfo {},
        };
        let json_response = serde_json::to_string(&response)
            .map_err(|e| UnstakingError::Other(format!("Failed to serialize response: {}", e)))?;
        return Ok(json_response);
    }

    // Store data if validation passed
    let (user_address, utxo, _rune_amount) =
        unstake_helper::store_unstake_data_from_validation(&validation_details)
            .map_err(|e| UnstakingError::Other(format!("Failed to store unstake record: {}", e)))?;

    // Verify storage
    // TODO: Do we need this?
    let stored_records = state::get_user_unstake_records(&user_address);
    if !stored_records.iter().any(|r| r.utxo == utxo) {
        let response = CanisterResponse {
            signed_psbt: String::new(),
            error: Some("Failed to verify storage".to_string()),
            debug: DebugInfo {},
        };
        let json_response = serde_json::to_string(&response)
            .map_err(|e| UnstakingError::Other(format!("Failed to serialize response: {}", e)))?;
        return Ok(json_response);
    }

    // Create response
    let response = CanisterResponse {
        signed_psbt: BASE64_STANDARD.encode(&signed_psbt),
        error: None,
        debug: DebugInfo {},
    };

    // Serialize response to JSON
    let json_response = serde_json::to_string(&response)
        .map_err(|e| UnstakingError::Other(format!("Failed to serialize response: {}", e)))?;

    let psbt = Psbt::deserialize(&signed_psbt)
        .map_err(|e| UnstakingError::Other(format!("Could not deocde psbt {}", e)))?;

    let txid = psbt.extract_tx().txid().to_string();
    let rune_totals = validation_details.rune_totals;

    let timestamp = time() / 1e9 as u64;

    // Monitor the pending transactions so that we update the exhcange rate
    // components once a 6 confirmations have passed
    transactions_storage::insert_transaction(TxRecord {
        txid: txid.clone(),
        liq_amount: rune_totals.secondary_pool_liq_value,
        sliq_amount: rune_totals.pool_sliq_value,
        tx_type: TxTypeEnum::Unstake,
        timestamp,
    });

    // Keep track of usable liq utxos
    for liq_utxo in validation_details.canister_liq_utxos {
        let utxo = format!("{}:{}", txid, liq_utxo);

        // The canister only signs LIQ utxos on unstake, meaning that
        // all previous utxos should contain LIQ tokens
        let unstake_utxo = UnstakeUtxo {
            utxo: utxo.clone(),
            timestamp: time() / 1e9 as u64,
            prev_utxos: inputs.iter().map(|item| item.1.clone()).collect(),
        };

        // Store the new utxo records
        transactions_storage::add_unstake_utxo(&utxo, &unstake_utxo);
    }

    log!(
        LOG,
        "[Unstake] {} LIQ -> {} SLIQ {} ",
        rune_totals.pool_liq_value,
        rune_totals.user_liq_value,
        txid
    );

    Ok(json_response)
}

// Get LIQ utxos that need to be signed by the canister
async fn get_inputs_to_sign(decoded: &[u8]) -> Result<Vec<(usize, String)>, UnstakingError> {
    let psbt = deserialize_psbt(decoded)?;
    let utxos = psbt
        .unsigned_tx
        .input
        .iter()
        .map(|item| item.previous_output.to_string())
        .collect::<Vec<String>>();

    let ord_client = get_ord_client();

    let utxo_info_map = ord_client
        .get_rune_utxo_info_map(&utxos)
        .await
        .expect("Could not fetch utxo info");

    let rune_id = default_config().bitcoin.liq_rune_id;

    let liq_utxos = utxo_info_map
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

    let liq_utxos: Vec<(usize, String)> = psbt
        .unsigned_tx
        .input
        .iter()
        .enumerate()
        .filter_map(|item| {
            if liq_utxos.contains(&item.1.previous_output.to_string()) {
                Some((item.0, item.1.previous_output.to_string()))
            } else {
                None
            }
        })
        .collect();

    if liq_utxos.is_empty() {
        return Err(UnstakingError::SigningError(
            "No LIQ inputs to tsign".to_string(),
        ));
    }

    // Check that the liq inputs are processed and available
    for liq_utxo in &liq_utxos {
        if !transactions_storage::contains_unstake_utxo(&liq_utxo.1) {
            return Err(UnstakingError::SigningError(format!(
                "LIQ input {} was not found in canister set",
                liq_utxo.1
            )));
        }
    }

    Ok(liq_utxos)
}
