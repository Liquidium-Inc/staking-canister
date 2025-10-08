use crate::{
    state::{self},
    validation::unstake::ValidationDetails,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UnstakeHelperError {
    #[error("No sLIQ runes sent to pool")]
    NoSLiqToPool,

    #[error("No user address found")]
    NoUserAddress,

    #[error("No UTXO found")]
    NoUtxo,
}

/// Stores unstake data from validation
pub fn store_unstake_data_from_validation(
    validation_details: &ValidationDetails,
) -> Result<(String, String, u128), UnstakeHelperError> {
    // Validate user address
    let user_address = validation_details
        .user_address
        .as_ref()
        .ok_or(UnstakeHelperError::NoUserAddress)?
        .clone();

    // Validate rune amount
    let rune_amount = validation_details.rune_totals.pool_sliq_value;
    if rune_amount == 0 {
        return Err(UnstakeHelperError::NoSLiqToPool);
    }

    // Validate UTXO
    let utxo = validation_details
        .secondary_pool_utxo
        .as_ref()
        .unwrap_or(&validation_details.rune_totals.sliq_id)
        .clone();

    if utxo.is_empty() {
        return Err(UnstakeHelperError::NoUtxo);
    }

    // Store the unstake record
    state::store_unstake_record(user_address.clone(), utxo.clone(), validation_details.rune_totals.secondary_pool_liq_value);

    // Return the data
    Ok((user_address, utxo, rune_amount))
}
