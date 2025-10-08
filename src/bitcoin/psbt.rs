//! PSBT handling and signing functionality.

use crate::bitcoin::{address, utils::script::get_script_type, utils::script::ScriptType};
use crate::state;
use bitcoin::Witness;
use bitcoin::{
    bip32::{ChildNumber, DerivationPath},
    hashes::Hash,
    psbt::Psbt,
    secp256k1::{self, Message},
    sighash::{EcdsaSighashType, SighashCache},
    PubkeyHash, PublicKey, ScriptBuf,
};
use ic_cdk::management_canister::{EcdsaCurve, EcdsaKeyId, SignWithEcdsaArgs};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PsbtError {
    #[error("Invalid PSBT: {0}")]
    InvalidPsbt(String),

    #[error("Missing witness_utxo for input {0}")]
    MissingWitnessUtxo(usize),

    #[error("Invalid pubkey hash: {0}")]
    InvalidPubkeyHash(String),

    #[error("Sighash error: {0}")]
    SighashError(String),

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("ECDSA sign failed: {0}")]
    EcdsaSignFailed(String),

    #[error("Invalid secp256k1 public key: {0}")]
    InvalidSecp256k1PublicKey(String),

    #[error("Invalid compact signature: {0}")]
    InvalidCompactSignature(String),

    #[error("Address error: {0}")]
    AddressError(String),
}

impl From<address::AddressError> for PsbtError {
    fn from(err: address::AddressError) -> Self {
        PsbtError::AddressError(err.to_string())
    }
}

/// Signs PSBT with canister wallet
///
/// * `input_psbt` - The PSBT to sign
/// * `key_index` - Optional key index to use for signing (0 for primary pool, 1 for secondary pool)
pub async fn sign(
    input_psbt: &[u8],
    key_index: Option<u32>,
    inputs_to_sign: Option<Vec<usize>>,
) -> Result<Vec<u8>, PsbtError> {
    // Deserialize PSBT
    let mut psbt = deserialize_psbt(input_psbt)?;

    // Fetch public key using the provided key_index
    let public_key: PublicKey = address::get_public_key(key_index).await?;
    let key_id = EcdsaKeyId {
        curve: EcdsaCurve::Secp256k1,
        name: state::get_key_name(),
    };

    // Set derivation path format based on key_index
    let index = key_index.unwrap_or(0);

    // Create little-endian encoded derivation path
    let mut path = vec![0u8; 4];
    path[0] = (index & 0xFF) as u8;
    path[1] = ((index >> 8) & 0xFF) as u8;
    path[2] = ((index >> 16) & 0xFF) as u8;
    path[3] = ((index >> 24) & 0xFF) as u8;
    let derivation_path = vec![path];

    // Initialize sighash cache
    let mut cache = SighashCache::new(&psbt.unsigned_tx);

    // Create BIP-44 derivation path
    let dp = DerivationPath::from(vec![
        ChildNumber::from_hardened_idx(44).unwrap(),
        ChildNumber::from_hardened_idx(0).unwrap(),
        ChildNumber::from_hardened_idx(0).unwrap(),
        ChildNumber::from_normal_idx(0).unwrap(),
        ChildNumber::from_normal_idx(index).unwrap(),
    ]);

    // Get xpub and master fingerprint
    let xpub = address::get_xpub(key_index).await?;
    let master_fingerprint = xpub.fingerprint();

    // Add xpub to PSBT
    psbt.xpub
        .insert(xpub.clone(), (master_fingerprint, dp.clone()));

    for (i, input) in psbt.inputs.iter_mut().enumerate() {
        
        // Skip third party inputs
        if let Some(inputs_to_sign) = &inputs_to_sign {
            if !inputs_to_sign.contains(&i) {
                continue;
            }
        }

        // Skip already signed inputs
        if input.final_script_witness.is_some()
            || !input.partial_sigs.is_empty()
            || input.tap_key_sig.is_some()
            || !input.tap_script_sigs.is_empty()
        {
            continue;
        }

        // Get UTXO
        let utxo = input
            .witness_utxo
            .as_ref()
            .ok_or_else(|| PsbtError::MissingWitnessUtxo(i))?;

        // Get script type
        let script_type = get_script_type(&utxo.script_pubkey);

        match script_type {
            ScriptType::P2WPKH => {
                // Extract pubkey hash
                let script_bytes = utxo.script_pubkey.as_bytes();
                let pk_hash = PubkeyHash::from_slice(&script_bytes[2..])
                    .map_err(|e| PsbtError::InvalidPubkeyHash(format!("{:?}", e)))?;

                let script_code = ScriptBuf::new_p2pkh(&pk_hash);

                // Compute SegWit signature hash
                let sighash = cache
                    .segwit_signature_hash(i, &script_code, utxo.value, EcdsaSighashType::All)
                    .map_err(|e| PsbtError::SighashError(format!("{:?}", e)))?;
                let msg = Message::from_slice(sighash.as_ref())
                    .map_err(|e| PsbtError::InvalidMessage(format!("{:?}", e)))?;

                // Sign hash with ECDSA
                let sig = ic_cdk::management_canister::sign_with_ecdsa(&SignWithEcdsaArgs {
                    message_hash: msg.as_ref().to_vec(),
                    derivation_path: derivation_path.clone(),
                    key_id: key_id.clone(),
                })
                .await
                .map_err(|e| PsbtError::EcdsaSignFailed(format!("{:?}", e)))?
                .signature;

                // Convert to secp256k1 public key
                let secp_pubkey = secp256k1::PublicKey::from_slice(&public_key.to_bytes())
                    .map_err(|e| PsbtError::InvalidSecp256k1PublicKey(format!("{:?}", e)))?;

                // Convert to Bitcoin PublicKey
                let btc_pubkey = PublicKey {
                    compressed: true,
                    inner: secp_pubkey,
                };

                // Create compact signature
                let secp_sig = secp256k1::ecdsa::Signature::from_compact(&sig)
                    .map_err(|e| PsbtError::InvalidCompactSignature(format!("{:?}", e)))?;

                // Add sighash type
                let btc_sig = bitcoin::ecdsa::Signature {
                    sig: secp_sig,
                    hash_ty: EcdsaSighashType::All,
                };

                // Add signature to PSBT
                input.partial_sigs.insert(btc_pubkey.clone(), btc_sig);

                // Add BIP32 derivation
                input
                    .bip32_derivation
                    .insert(secp_pubkey, (master_fingerprint, dp.clone()));

                input.witness_script = None;

                // Build the final witness
                let mut witness = Witness::new();
                witness.push_bitcoin_signature(&secp_sig.serialize_der(), EcdsaSighashType::All);
                witness.push(&btc_pubkey.to_bytes());

                input.final_script_witness = Some(witness);
            }
            // Skip non-P2WPKH inputs
            _ => {
                continue;
            }
        }
    }

    // Serialize PSBT
    Ok(serialize_psbt(&psbt))
}

/// Deserializes PSBT
pub fn deserialize_psbt(psbt_bytes: &[u8]) -> Result<Psbt, PsbtError> {
    Psbt::deserialize(psbt_bytes).map_err(|e| PsbtError::InvalidPsbt(format!("{:?}", e)))
}

/// Serializes PSBT
pub fn serialize_psbt(psbt: &Psbt) -> Vec<u8> {
    psbt.serialize()
}
