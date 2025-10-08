use crate::state::{self};
use base64::{prelude::BASE64_STANDARD, Engine};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WithdrawHelperError {
    #[error("Failed to parse runes: {0}")]
    RuneParseError(String),

    #[error("No user address found")]
    NoUserAddress,

    #[error("No UTXO found")]
    NoUtxo,

    #[error("{0}")]
    Other(String),
}

/// Extracts withdrawal data from a PSBT
pub async fn extract_withdraw_data(
    psbt_base64: &str,
) -> Result<(String, String, u128), WithdrawHelperError> {
    // Parse runes from PSBT
    let (rune_data, _debug_output) =
        ordinals_runes::parse_psbt_runes_with_debug_legacy(psbt_base64)
            .map_err(|e| WithdrawHelperError::RuneParseError(e.to_string()))?;

    // Get pool address and rune ID
    let secondary_pool_address = state::get_pool_address(1);
    let app_config = crate::config::default_config();
    let liq_rune_id = app_config.bitcoin.liq_rune_id.clone();

    let mut user_address: Option<String> = None;
    let mut utxo: Option<String> = None;

    // Decode PSBT
    let psbt_bytes = BASE64_STANDARD
        .decode(psbt_base64)
        .map_err(|e| WithdrawHelperError::Other(format!("Failed to decode base64: {}", e)))?;

    let psbt = crate::bitcoin::psbt::deserialize_psbt(&psbt_bytes)
        .map_err(|e| WithdrawHelperError::Other(format!("Failed to deserialize PSBT: {}", e)))?;

    // Find user address in transaction outputs
    // Commented by Bogdan, We already check for the user address by iterating over the edicts
    // for output in psbt.unsigned_tx.output.iter() {
    //     if let Some(address) =
    //         bitcoin::Address::from_script(&output.script_pubkey, crate::state::get_btc_network())
    //             .ok()
    //     {
    //         let addr_str = address.to_string();
    //         // Skip pool address and OP_RETURN outputs
    //         if addr_str != secondary_pool_address && !output.script_pubkey.is_op_return() {
    //             user_address = Some(addr_str);
    //             break;
    //         }
    //     }
    // }
    
    let mut rune_amount = 0;
    // Extract data from edicts
    for edict in &rune_data.edicts {
        if (edict.output as usize) >= psbt.outputs.len() {
            return Err(WithdrawHelperError::RuneParseError(
                "bug: invalid tx".to_string(),
            ));
        }
        if let Some(address) = &edict.address {
            // Check user receiving runes
            if address != &secondary_pool_address
                && edict.id == liq_rune_id
                && user_address.is_none()
            {
                if rune_amount > 0 {
                    return Err(WithdrawHelperError::RuneParseError(
                        "Invalid tx structure".to_string(),
                    ));
                }

                user_address = Some(address.clone());
                rune_amount = edict.amount;
            }

            // This is invalid utxo holds tx hash not tx index
            // Extract UTXO for secondary pool
            // if address == &secondary_pool_address && edict.id == sliq_rune_id {
            //     utxo = Some(format!("{}:{}", edict.id, edict.output));
            // }
        }
    }

    if rune_amount == 0 {
        return Err(WithdrawHelperError::RuneParseError(
            "Invalid tx structure".to_string(),
        ));
    }

    // If user address not found, check inputs
    if user_address.is_none() {
        for input in psbt.inputs.iter() {
            if let Some(witness_utxo) = &input.witness_utxo {
                if let Some(address) = bitcoin::Address::from_script(
                    &witness_utxo.script_pubkey,
                    crate::state::get_btc_network(),
                )
                .ok()
                {
                    let addr_str = address.to_string();
                    if addr_str != secondary_pool_address {
                        user_address = Some(addr_str);
                        break;
                    }
                }
            }
        }
    }

    // If UTXO not found, try to get from inputs
    if utxo.is_none() && !psbt.unsigned_tx.input.is_empty() {
        // Default to input 0
        let input = &psbt.unsigned_tx.input[0];
        utxo = Some(format!(
            "{}:{}",
            input.previous_output.txid, input.previous_output.vout
        ));

        // Check all inputs for secondary pool address
        for (i, input) in psbt.unsigned_tx.input.iter().enumerate() {
            if let Some(witness_utxo) = &psbt.inputs[i].witness_utxo {
                if let Some(address) = bitcoin::Address::from_script(
                    &witness_utxo.script_pubkey,
                    crate::state::get_btc_network(),
                )
                .ok()
                {
                    if address.to_string() == secondary_pool_address {
                        utxo = Some(format!(
                            "{}:{}",
                            input.previous_output.txid, input.previous_output.vout
                        ));
                    }
                }
            }
        }
    }

    // Validate required data
    let user_addr = user_address.ok_or(WithdrawHelperError::NoUserAddress)?;
    let utxo_str = utxo.ok_or(WithdrawHelperError::NoUtxo)?;

    Ok((user_addr, utxo_str, rune_amount))
}
