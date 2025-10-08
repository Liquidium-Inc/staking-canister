//! Storage management for unstaking operations.

use crate::types::account::Account;
use crate::types::cbor_wrapper::Cbor;
use candid::CandidType;
use ic_stable_structures::{memory_manager::MemoryId, StableBTreeMap, StableCell};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;

use crate::state::{MemoryIndex, MEMORY_MANAGER, VM};

/// Structure to hold unstaking data
#[derive(Debug, Clone, Serialize, Deserialize, CandidType)]
pub struct UnstakeRecord {
    /// User address that initiated the unstake
    pub user_address: String,

    /// UTXO generated in the secondary pool
    pub utxo: String,

    /// Amount of runes transferred from primary to secondary pool
    pub rune_amount: u128,

    /// Timestamp when the unstake was performed
    pub timestamp: u64,
}

thread_local! {
    /// Storage for unstaking records
    /// Key: (User address, Record Id)
    /// Value: Vector of unstaking records for that user
    pub static UNSTAKE_RECORDS: RefCell<StableBTreeMap<(Account, u128), Cbor<UnstakeRecord>, VM>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::UnstakeRecordsMemoryId as u8))),
        )
    );

    /// Counter used to keep track of unstake records
    pub static UNSTAKE_RECORDS_COUNTER: RefCell<ic_stable_structures::StableCell<u128, VM>> = RefCell::new(
        StableCell::init(
                MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::UnstakeStorageCounterMemoryId as u8))),
                0u128
            )
            .expect(concat!("Failed to init stable cell: ", stringify!(UNSTAKE_RECORDS_COUNTER))),
    );
}

/// Stores an unstaking record
pub fn store_unstake_record(user_address: String, utxo: String, rune_amount: u128) {
    // Get current timestamp (nanoseconds to seconds)
    let timestamp = ic_cdk::api::time() / 1_000_000_000;

    // Create the record
    let record = UnstakeRecord {
        user_address: user_address.clone(),
        utxo: utxo.clone(),
        rune_amount,
        timestamp,
    };

    let id: Account = user_address.into();
    let record_id = get_unstake_record_next_id();
    // Store the record
    UNSTAKE_RECORDS.with_borrow_mut(|records| {
        records.insert((id, record_id), Cbor(record));
    })
}

/// Gets all unstaking records for a user
pub fn get_user_unstake_records(user_address: &str) -> Vec<UnstakeRecord> {
    let target_user_id = Account::from(user_address.to_string());
    UNSTAKE_RECORDS.with_borrow(|records| {
        records
            .range((target_user_id.clone(), 0u128)..)
            .take_while(|((user_account, _), _)| target_user_id == *user_account)
            .map(|((_, _), record)| record.0.clone())
            .collect::<Vec<UnstakeRecord>>()
    })
}

/// Gets all unstaking records
pub fn get_all_unstake_records() -> HashMap<String, Vec<UnstakeRecord>> {
    let mut result = HashMap::new();
    UNSTAKE_RECORDS.with_borrow(|records| {
        records.iter().for_each(|item| {
            let result = result.entry(item.0 .0.to_string()).or_insert_with(Vec::new);
            result.push(item.1 .0.clone());
        });
    });

    result
}

/// Gets the most recent unstaking record for a user
pub fn get_latest_unstake_record(user_address: &str) -> Option<UnstakeRecord> {
    let user_records = get_user_unstake_records(user_address);
    user_records
        .iter()
        .max_by_key(|record| record.timestamp)
        .cloned()
}

pub fn get_unstake_record_next_id() -> u128 {
    UNSTAKE_RECORDS_COUNTER.with(|cell| {
        let mut cell = cell.borrow_mut();
        let current = cell.get().clone();
        let incremented = current + 1u128;
        cell.set(incremented).expect("Failed to increment counter");
        current
    })
}
