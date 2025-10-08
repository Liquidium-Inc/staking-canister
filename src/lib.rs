//! Core implementation of the Liquidium staking canister.

mod bitcoin;
mod config;
mod core;
mod libs;
mod log;
mod oracle;
mod state;
mod types;
mod validation;

use std::time::Duration;

use ic_cdk::api::{is_controller, msg_caller};
use ic_cdk::export_candid;
use ic_cdk_macros::{init, post_upgrade, pre_upgrade, query, update};

use crate::config::{default_config, AppConfig};
use crate::core::transaction_parser::{self};
use crate::log::{get_errors, get_logs};
use crate::state::transactions_storage::{
    TxRecord, UnstakeUtxo, AVAILABLE_UNSTAKE_UTXOS, LAST_BLOCK, PROCESSED_TRANSACTIONS,
    TRANSACTION_RECORDS,
};
use crate::state::{allowed, UnstakeRecord, PENDING_REWARDS};
use crate::types::{FingerprintInfo, PoolInfo};

// Pre-generates pool data (address, xpub, fingerprint) for common indices
async fn initialize_pool_data() {
    for index in 0..=1 {
        if !state::has_pool_data(index) {
            // Generate address
            if let Ok(address) = bitcoin::generate_bitcoin_address(Some(index)).await {
                state::set_pool_address(address, index);
            }

            // Generate xpub
            if let Ok(xpub) = bitcoin::get_xpub(Some(index)).await {
                state::set_pool_xpub(format!("{}", xpub), index);
            }

            // Generate fingerprint
            if let Ok(fingerprint) = bitcoin::get_fingerprint(Some(index)).await {
                state::set_pool_fingerprint(format!("{:x}", fingerprint), index);
            }
        }
    }
}

#[init]
pub fn init() {
    state::init();

    // Initialize the pool state
    ic_cdk_timers::set_timer(Duration::from_secs(0), || {
        ic_cdk::futures::spawn(async {
            initialize_pool_data().await;
        });
    });
}

#[update(hidden = true)]
pub async fn set_mempool_url(url: String) {
    assert!(is_controller(&msg_caller()));
    state::set_mempool_url(Some(url));
}

/// Returns pool address for specified derivation index
#[query(guard = "no_replicated_call")]
pub fn get_pool_address(index: Option<u32>) -> String {
    let index_value = index.unwrap_or(0);
    state::get_pool_address(index_value)
}

/// Returns fingerprint of the public key
#[query(guard = "no_replicated_call")]
pub fn get_fingerprint(index: Option<u32>) -> FingerprintInfo {
    let index_value = index.unwrap_or(0);
    let fingerprint = state::get_pool_fingerprint(index_value);

    FingerprintInfo { fingerprint }
}

/// Returns extended public key (xpub)
#[query(guard = "no_replicated_call")]
pub fn get_xpub(index: Option<u32>) -> String {
    let index_value = index.unwrap_or(0);
    state::get_pool_xpub(index_value)
}

/// Returns combined pool information for specified index
#[query(guard = "no_replicated_call")]
pub fn get_pool(index: Option<u32>) -> PoolInfo {
    let index_value = index.unwrap_or(0);

    let address = state::get_pool_address(index_value);
    let xpub = state::get_pool_xpub(index_value);
    let fingerprint = state::get_pool_fingerprint(index_value);

    PoolInfo {
        address,
        xpub,
        fingerprint,
        index: index_value,
    }
}

/// Signs PSBT with canister's Bitcoin wallet
#[update]
pub async fn stake(input_psbt_base64: String) -> String {
    allowed().await.expect("Unauthorized");
    match core::staking::stake(input_psbt_base64).await {
        Ok(response_json) => response_json,
        Err(e) => format!("Error: {}", e),
    }
}

/// Unstakes tokens from the pool
#[update]
pub async fn unstake(input_psbt_base64: String) -> String {
    allowed().await.expect("Unauthorized");
    match core::unstaking::unstake(input_psbt_base64).await {
        Ok(signed_psbt) => signed_psbt,
        Err(e) => format!("Error: {}", e),
    }
}

/// Withdraws tokens from the pool
#[update]
pub async fn withdraw(input_psbt_base64: String) -> String {
    allowed().await.expect("Unauthorized");

    match core::withdrawal::withdraw(input_psbt_base64).await {
        Ok(signed_psbt) => signed_psbt,
        Err(e) => format!("Error: {}", e),
    }
}

/// Returns current exchange rate between LIQ and sLIQ tokens
#[query(guard = "no_replicated_call")]
pub fn get_exchange_rate() -> Result<f64, String> {
    state::get_stored_exchange_rate().ok_or_else(|| {
        "Exchange rate not available. Please update exchange rate components first.".to_string()
    })
}

/// Returns the stored circulating supply and balance values
#[query(guard = "no_replicated_call")]
pub fn get_exchange_rate_components() -> Result<(Option<u128>, Option<u128>), String> {
    Ok((
        state::get_stored_circulating_supply(),
        state::get_stored_balance(),
    ))
}

/// Optional | Manually initializes pool data (use only if the canister has already been deployed)
/// The init function initializes the pool once deployed the first time.
#[update]
pub async fn initialize_pool_addresses_range(
    start_index: u32,
    end_index: u32,
) -> Result<String, String> {
    allowed().await.expect("Unauthorized");
    if end_index < start_index {
        return Err("End index must be greater than or equal to start index".to_string());
    }

    if end_index - start_index > 10 {
        return Err("Range too large, maximum 10 addresses at once".to_string());
    }

    let mut initialized = Vec::new();

    for index in start_index..=end_index {
        if !state::has_pool_data(index) {
            let mut status = Vec::new();

            // Generate address
            if let Ok(address) = bitcoin::generate_bitcoin_address(Some(index)).await {
                state::set_pool_address(address.clone(), index);
                status.push(format!("address: {}", address));
            }

            // Generate xpub
            if let Ok(xpub) = bitcoin::get_xpub(Some(index)).await {
                let xpub_str = format!("{}", xpub);
                state::set_pool_xpub(xpub_str.clone(), index);
                status.push(format!("xpub: {}...", &xpub_str[..20]));
            }

            // Generate fingerprint
            if let Ok(fingerprint) = bitcoin::get_fingerprint(Some(index)).await {
                let fingerprint_str = format!("{:x}", fingerprint);
                state::set_pool_fingerprint(fingerprint_str.clone(), index);
                status.push(format!("fingerprint: {}", fingerprint_str));
            }

            initialized.push(format!("Index {}: {}", index, status.join(", ")));
        } else {
            initialized.push(format!("Index {}: already exists", index));
        }
    }

    Ok(format!(
        "Initialized pool data:\n{}",
        initialized.join("\n")
    ))
}

/// Pre-upgrade hook to save state before canister upgrade
#[pre_upgrade]
fn pre_upgrade() {
    core::storage::pre_upgrade();
}

/// Post-upgrade hook to restore state after canister upgrade
#[post_upgrade]
fn post_upgrade() {
    core::storage::post_upgrade();
    transaction_parser::init();
}

#[query(guard = "no_replicated_call")]
pub fn get_processing() -> Vec<String> {
    PROCESSED_TRANSACTIONS.with_borrow(|pt| pt.keys().collect())
}

#[query(guard = "no_replicated_call")]
pub fn latest_block() -> Option<u128> {
    LAST_BLOCK.with_borrow(|block| *block.get())
}

#[query(guard = "no_replicated_call")]
pub fn get_recorded() -> Vec<TxRecord> {
    TRANSACTION_RECORDS.with_borrow(|tr| tr.values().map(|item| item.0.clone()).collect())
}

#[query(guard = "no_replicated_call")]
pub fn get_unstake_utxos() -> Vec<UnstakeUtxo> {
    AVAILABLE_UNSTAKE_UTXOS.with_borrow(|tr| tr.values().map(|item| item.0.clone()).collect())
}

#[query(guard = "no_replicated_call")]
pub fn get_config() -> AppConfig {
    default_config()
}

#[query(guard = "no_replicated_call")]
pub fn logs() -> Vec<String> {
    get_logs()
}

#[query(guard = "no_replicated_call")]
pub fn errors() -> Vec<String> {
    get_errors()
}

#[query(guard = "no_replicated_call")]
pub fn pending_rewards() -> u128 {
    PENDING_REWARDS.with_borrow(|r| r.get().unwrap_or(0))
}

/// Debug | Returns the last 10 unstake records across all users
#[query(guard = "no_replicated_call")]
pub fn get_recent_unstake_records() -> Vec<UnstakeRecord> {
    // Get all unstake records
    let all_records = state::get_all_unstake_records();

    // Flatten, sort by timestamp (newest first), and limit to 10
    let mut flattened_records: Vec<UnstakeRecord> = all_records
        .into_values()
        .flat_map(|records| records)
        .collect();

    flattened_records.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    flattened_records.truncate(10);

    flattened_records
}

/// Debug | Returns the latest unstake record for a specific user
#[query(guard = "no_replicated_call")]
pub fn get_user_latest_unstake_record(user_address: String) -> Option<UnstakeRecord> {
    state::get_latest_unstake_record(&user_address)
}

fn no_replicated_call() -> Result<(), String> {
    if ic_cdk::api::in_replicated_execution() {
        return Err("Not allowed".to_string());
    }
    Ok(())
}

#[cfg(feature = "dev-hooks")]
pub mod dev_hooks;

export_candid!();

#[cfg(test)]
mod tests;
