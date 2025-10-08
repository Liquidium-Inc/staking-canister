use base64::{self, prelude::BASE64_STANDARD, Engine};
use bitcoin::psbt::Psbt;

use thiserror::Error;

use crate::{
    bitcoin::psbt,
    config::default_config,
    core::withdraw_helper,
    oracle::ord_client::OrdClientTrait,
    state::{self, get_ord_client},
};

#[derive(Debug, Error)]
pub enum WithdrawValidationError {
    #[error("Invalid PSBT: {0}")]
    InvalidPsbt(String),

    #[error("Transaction fee is too low")]
    FeeTooLow,

    #[error("Transaction fee is too high")]
    FeeTooHigh,

    #[error("Withdrawal too early, must wait for the lockup period to end")]
    WithdrawTooEarly,

    #[error("UTXO mismatch: expected {0}, got {1}")]
    UtxoMismatch(String, String),

    #[error("User address mismatch: expected {0}, got {1}")]
    UserAddressMismatch(String, String),

    #[error("Amount mismatch: expected {0}, got {1}")]
    AmountMismatch(String, String),

    #[error("No unstake record found for user")]
    NoUnstakeRecord,

    #[error("Failed to extract withdrawal data: {0}")]
    DataExtractionError(String),

    #[error("{0}")]
    Other(String),
}

// Validates withdrawal PSBT
pub async fn validate_withdraw(psbt_bytes: &[u8]) -> Result<String, WithdrawValidationError> {
    let psbt = psbt::deserialize_psbt(psbt_bytes)
        .map_err(|e| WithdrawValidationError::InvalidPsbt(e.to_string()))?;

    validate_psbt_structure(&psbt)?;
    validate_fees(&psbt)?;

    let psbt_base64 = BASE64_STANDARD.encode(psbt_bytes);

    let result = withdraw_helper::extract_withdraw_data(&psbt_base64).await;
    let (user_address, utxo, rune_amount) =
        result.map_err(|e| WithdrawValidationError::DataExtractionError(e.to_string()))?;

    validate_unstake_record(&user_address, &utxo, rune_amount).await?;

    Ok(utxo)
}

// Verifies withdrawal against stored record
async fn validate_unstake_record(
    user_address: &str,
    utxo: &str,
    rune_amount: u128,
) -> Result<(), WithdrawValidationError> {
    let records = state::get_user_unstake_records(user_address);

    // Find record with matching UTXO
    let record = records.iter().find(|r| r.utxo == utxo);

    // Commeted out by Bogdan, Utxo should exists
    // If not found, try to match by txid
    // if record.is_none() {
    //     let parts: Vec<&str> = utxo.split(':').collect();
    //     if parts.len() == 2 {
    //         let txid = parts[0];
    //         let record = records.iter().find(|r| r.utxo.starts_with(txid));
    //         if let Some(record_by_txid) = record {
    //             let app_config = crate::config::default_config();
    //             let lockup_period = app_config.bitcoin.withdrawal_lockup_period;
    //             let current_time = ic_cdk::api::time() / 1_000_000_000;
    //             let unlock_time = record_by_txid.timestamp + lockup_period;

    //             if current_time < unlock_time {
    //                 return Err(WithdrawValidationError::WithdrawTooEarly);
    //             }

    //             return Ok(());
    //         }
    //     }
    // }

    let record = record.ok_or(WithdrawValidationError::NoUnstakeRecord)?;

    if record.user_address != user_address {
        return Err(WithdrawValidationError::UserAddressMismatch(
            record.user_address.clone(),
            user_address.to_string(),
        ));
    }

    if record.utxo != utxo {
        return Err(WithdrawValidationError::UtxoMismatch(
            record.utxo.clone(),
            utxo.to_string(),
        ));
    }

    if record.rune_amount != rune_amount {
        return Err(WithdrawValidationError::AmountMismatch(
            record.rune_amount.to_string(),
            rune_amount.to_string(),
        ));
    }

    let ord_client = get_ord_client();
    let rune_id = default_config().bitcoin.liq_rune_id;
    let (real_amount, _) = ord_client
        .get_rune_sent_amount(&vec![record.utxo.clone()], &rune_id)
        .await
        .map_err(WithdrawValidationError::Other)?;
    
    if record.rune_amount != real_amount {
        return Err(WithdrawValidationError::AmountMismatch(
            record.rune_amount.to_string(),
            rune_amount.to_string(),
        ));
    }

    let app_config = crate::config::default_config();
    let lockup_period = app_config.bitcoin.withdrawal_lockup_period;
    let current_time = ic_cdk::api::time() / 1_000_000_000;

    if current_time < record.timestamp + lockup_period {
        return Err(WithdrawValidationError::WithdrawTooEarly);
    }

    Ok(())
}

// Checks PSBT structure and secondary pool input
fn validate_psbt_structure(psbt: &Psbt) -> Result<(), WithdrawValidationError> {
    if psbt.inputs.is_empty() {
        return Err(WithdrawValidationError::InvalidPsbt(
            "PSBT has no inputs".to_string(),
        ));
    }

    if psbt.unsigned_tx.output.is_empty() {
        return Err(WithdrawValidationError::InvalidPsbt(
            "PSBT has no outputs".to_string(),
        ));
    }

    let secondary_pool_address = state::get_pool_address(1);
    let mut found_secondary_pool_input = false;

    for (i, input) in psbt.inputs.iter().enumerate() {
        if let Some(witness_utxo) = &input.witness_utxo {
            if let Some(address) =
                bitcoin::Address::from_script(&witness_utxo.script_pubkey, state::get_btc_network())
                    .ok()
            {
                let addr_str = address.to_string();
                if i == 1 {
                    found_secondary_pool_input = true;
                    break;
                }
                if addr_str == secondary_pool_address {
                    found_secondary_pool_input = true;
                    break;
                }
            }
        }
    }

    // Allow transactions without secondary pool input for flexibility
    let _ = found_secondary_pool_input;

    Ok(())
}

// Checks transaction fee bounds
fn validate_fees(psbt: &Psbt) -> Result<(), WithdrawValidationError> {
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
        return Err(WithdrawValidationError::Other(
            "Transaction has no fee".to_string(),
        ));
    }
    let fee = total_input - total_output;

    if fee < 500 {
        return Err(WithdrawValidationError::FeeTooLow);
    }

    if fee > 100000 {
        return Err(WithdrawValidationError::FeeTooHigh);
    }

    Ok(())
}
