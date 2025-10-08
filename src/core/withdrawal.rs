use crate::types::{CanisterResponse, DebugInfo};
use crate::{
    bitcoin::psbt::{self, PsbtError},
    validation::withdraw::{validate_withdraw, WithdrawValidationError},
};
use base64::prelude::BASE64_STANDARD;
use base64::{self, Engine};
use serde_json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WithdrawalError {
    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Signing error: {0}")]
    SigningError(String),

    #[error("{0}")]
    Other(String),
}

impl From<WithdrawValidationError> for WithdrawalError {
    fn from(err: WithdrawValidationError) -> Self {
        WithdrawalError::ValidationError(err.to_string())
    }
}

impl From<PsbtError> for WithdrawalError {
    fn from(err: PsbtError) -> Self {
        WithdrawalError::SigningError(err.to_string())
    }
}

/// Validates and signs a withdrawal PSBT
pub async fn withdraw(input_psbt_base64: String) -> Result<String, WithdrawalError> {
    let mut error_msg: Option<String> = None;

    if input_psbt_base64.len() > 100_000 {
        return Err(WithdrawalError::ValidationError(
            "Psbt is to large".to_string(),
        ));
    }

    // Decode base64 input
    let decoded = BASE64_STANDARD
        .decode(&input_psbt_base64)
        .map_err(|e| WithdrawalError::Other(format!("Failed to decode base64 input: {}", e)))?;

    // Validate PSBT
    if let Err(err) = validate_withdraw(&decoded).await {
        let response = CanisterResponse {
            signed_psbt: String::new(),
            error: Some(err.to_string()),
            debug: DebugInfo {},
        };

        let json_response = serde_json::to_string(&response)
            .map_err(|e| WithdrawalError::Other(format!("Failed to serialize response: {}", e)))?;

        return Ok(json_response);
    }

    // Sign with key_index 1 (secondary pool)
    let signed_psbt = match psbt::sign(&decoded, Some(1), Some(vec![0])).await {
        Ok(signed) => BASE64_STANDARD.encode(&signed),
        Err(err) => {
            error_msg = Some(err.to_string());
            String::new()
        }
    };

    let response = CanisterResponse {
        signed_psbt,
        error: error_msg,
        debug: DebugInfo {},
    };

    let json_response = serde_json::to_string(&response)
        .map_err(|e| WithdrawalError::Other(format!("Failed to serialize response: {}", e)))?;

    Ok(json_response)
}
