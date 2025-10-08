use bitcoin::{
    bip32::{ExtendedPubKey, Fingerprint},
    Address, PublicKey,
};
use ic_cdk::management_canister::{ecdsa_public_key, EcdsaKeyId, EcdsaPublicKeyArgs};
use secp256k1;
use thiserror::Error;

use crate::state;

#[derive(Debug, Error)]
pub enum AddressError {
    #[error("Failed to get public key: {0}")]
    PublicKeyError(String),

    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),

    #[error("Address error: {0}")]
    AddressError(String),

    #[error("Invalid secp256k1 public key: {0}")]
    InvalidSecp256k1Key(String),
}

// Creates derivation path from index using little-endian encoding
fn create_derivation_path(index: u32) -> Vec<Vec<u8>> {
    let mut path = vec![0u8; 4];
    path[0] = (index & 0xFF) as u8;
    path[1] = ((index >> 8) & 0xFF) as u8;
    path[2] = ((index >> 16) & 0xFF) as u8;
    path[3] = ((index >> 24) & 0xFF) as u8;
    vec![path]
}

// Gets ECDSA key ID from state configuration
fn get_ecdsa_key_id() -> EcdsaKeyId {
    let key_name = state::get_key_name();
    EcdsaKeyId {
        curve: ic_cdk::management_canister::EcdsaCurve::Secp256k1,
        name: key_name,
    }
}

/// Gets public key via threshold ECDSA
pub async fn get_public_key(index: Option<u32>) -> Result<PublicKey, AddressError> {
    let index = index.unwrap_or(0);
    let key_id = get_ecdsa_key_id();
    let derivation_path = create_derivation_path(index);

    let arg = EcdsaPublicKeyArgs {
        canister_id: None,
        derivation_path,
        key_id,
    };

    let response = ecdsa_public_key(&arg)
        .await
        .map_err(|e| AddressError::PublicKeyError(format!("{:?}", e)))?;

    PublicKey::from_slice(&response.public_key)
        .map_err(|e| AddressError::InvalidPublicKey(format!("{:?}", e)))
}

/// Creates Bitcoin address from pubkey
pub async fn generate_bitcoin_address(index: Option<u32>) -> Result<String, AddressError> {
    let index = index.unwrap_or(0);
    let public_key = get_public_key(Some(index)).await?;
    let btc_network = state::get_btc_network();

    let address = Address::p2wpkh(&public_key, btc_network)
        .map_err(|e| AddressError::AddressError(format!("{:?}", e)))?;

    Ok(address.to_string())
}

/// Builds extended public key
pub async fn get_xpub(index: Option<u32>) -> Result<ExtendedPubKey, AddressError> {
    let index = index.unwrap_or(0);
    let bitcoin_pubkey = get_public_key(Some(index)).await?;

    let secp_pubkey = secp256k1::PublicKey::from_slice(&bitcoin_pubkey.to_bytes())
        .map_err(|e| AddressError::InvalidSecp256k1Key(format!("{:?}", e)))?;

    let xpub = ExtendedPubKey {
        network: state::get_btc_network(),
        depth: if index > 0 { 1u8 } else { 0u8 },
        parent_fingerprint: Fingerprint::default(),
        child_number: bitcoin::bip32::ChildNumber::Normal { index },
        chain_code: [0; 32].into(),
        public_key: secp_pubkey,
    };

    Ok(xpub)
}

/// Gets fingerprint from public key
pub async fn get_fingerprint(index: Option<u32>) -> Result<Fingerprint, AddressError> {
    let xpub = get_xpub(index).await?;
    Ok(xpub.fingerprint())
}
