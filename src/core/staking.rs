use crate::log::LOG;
use crate::oracle::omnity::OmnityOrdClient;
use crate::state::apply_rewards;
use crate::state::transactions_storage::{self, TxRecord, TxTypeEnum, UnstakeUtxo};
use crate::types::{CanisterResponse, DebugInfo};
use crate::{
    bitcoin::psbt::{self, PsbtError},
    validation::stake::{validate_stake, StakeValidationError},
};
use base64;
use bitcoin::psbt::Psbt;
use ic_canister_log::log;
use ic_cdk::api::time;
use serde_json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StakingError {
    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Signing error: {0}")]
    SigningError(String),

    #[error("{0}")]
    Other(String),
}

impl From<StakeValidationError> for StakingError {
    fn from(err: StakeValidationError) -> Self {
        StakingError::ValidationError(err.to_string())
    }
}

impl From<PsbtError> for StakingError {
    fn from(err: PsbtError) -> Self {
        StakingError::SigningError(err.to_string())
    }
}

/// Validates and signs stake PSBT
pub async fn stake(input_psbt_base64: String) -> Result<String, StakingError> {
    // Apply rewards before staking
    apply_rewards();
    
    // Make sure that omnity is in sync
    OmnityOrdClient::check_sync(true)
        .await
        .map_err(|e| StakingError::Other(format!("Out of sync: {}", e)))?;

    // Validate PSBT
    let validation_result = validate_stake(&input_psbt_base64).await;

    match &validation_result {
        Ok((is_valid, validation_details)) => {
            if !is_valid {
                let response = CanisterResponse {
                    signed_psbt: String::new(),
                    error: Some(format!(
                        "Validation failed: {}",
                        validation_details.validation_result
                    )),
                    debug: DebugInfo {},
                };
                let json_response = serde_json::to_string(&response).map_err(|e| {
                    StakingError::Other(format!("Failed to serialize response: {}", e))
                })?;
                return Ok(json_response);
            }
        }
        Err(err) => {
            let response = CanisterResponse {
                signed_psbt: String::new(),
                error: Some(err.to_string()),
                debug: DebugInfo {},
            };
            let json_response = serde_json::to_string(&response)
                .map_err(|e| StakingError::Other(format!("Failed to serialize response: {}", e)))?;
            return Ok(json_response);
        }
    }

    let details = validation_result.expect("Expected validation result").1;
    // If validation passed, sign the PSBT
    match decode_and_sign_psbt(&input_psbt_base64, details.utxos_to_sign).await {
        Ok((signed_psbt_encoded, psbt)) => {
            let response = CanisterResponse {
                signed_psbt: signed_psbt_encoded,
                error: None,
                debug: DebugInfo {},
            };
            let json_response = serde_json::to_string(&response)
                .map_err(|e| StakingError::Other(format!("Failed to serialize response: {}", e)))?;

            let tx = psbt.extract_tx();
            let txid = tx.txid().to_string();

            let rune_totals = details.rune_totals;

            let timestamp = time() / 1e9 as u64;

            // Monitor the pending transactions so that we update the exhcange rate
            // components once 4 confirmations have passed
            transactions_storage::insert_transaction(TxRecord {
                txid: txid.clone(),
                liq_amount: rune_totals.pool_liq_value,
                sliq_amount: rune_totals.user_sliq_value,
                tx_type: TxTypeEnum::Stake,
                timestamp,
            });

            // Keep track of available unstake liq utxos
            for liq_utxo in details.canister_liq_utxos {
                let utxo = format!("{}:{}", txid, liq_utxo);
                let unstake_utxo = UnstakeUtxo {
                    utxo: utxo.clone(),
                    timestamp: time() / 1e9 as u64,
                    prev_utxos: vec![],
                };
                transactions_storage::add_unstake_utxo(&utxo, &unstake_utxo);
            }

            Ok(json_response)
        }
        Err(err) => {
            let response = CanisterResponse {
                signed_psbt: String::new(),
                error: Some(format!("Signing error: {}", err)),
                debug: DebugInfo {},
            };
            let json_response = serde_json::to_string(&response)
                .map_err(|e| StakingError::Other(format!("Failed to serialize response: {}", e)))?;
            Ok(json_response)
        }
    }
}

/// Signs PSBT with primary pool key
async fn decode_and_sign_psbt(
    psbt_base64: &str,
    inputs_to_sign: Vec<usize>,
) -> Result<(String, Psbt), StakingError> {
    use base64::prelude::*;

    let decoded = BASE64_STANDARD
        .decode(psbt_base64)
        .map_err(|e| StakingError::Other(format!("Failed to decode base64 input: {}", e)))?;

    let signed = psbt::sign(&decoded, Some(0), Some(inputs_to_sign)).await?;
    let signed_psbt = BASE64_STANDARD.encode(&signed);

    let psbt = Psbt::deserialize(&signed)
        .map_err(|e| StakingError::Other(format!("Could not deocde psb {}", e)))?;
    Ok((signed_psbt, psbt))
}
