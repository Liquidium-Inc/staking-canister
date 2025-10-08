pub mod cbor_wrapper;
pub mod identifier32;
pub mod account;

use candid::CandidType;
use serde::{Deserialize, Serialize};
/// PSBT input representation
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct PsbtInput {
    pub bytes: Vec<u8>,
}

/// Rune token configuration
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct RuneConfig {
    pub id: String,
    pub amount: u64,
}

/// Rune swap operation result
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct RuneSwapResult {
    pub transaction_bytes: Vec<u8>,
    pub success: bool,
    pub message: String,
}

/// Bitcoin address data
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct AddressInfo {
    pub address: String,
    pub network: String,
}

/// Bitcoin public key fingerprint
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct FingerprintInfo {
    pub fingerprint: String,
}

/// Combined Bitcoin pool information
#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct PoolInfo {
    pub address: String,
    pub xpub: String,
    pub fingerprint: String,
    pub index: u32,
}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct DebugInfo {}

#[derive(CandidType, Serialize, Deserialize, Clone, Debug)]
pub struct CanisterResponse {
    pub signed_psbt: String,
    pub error: Option<String>,
    pub debug: DebugInfo,
}
