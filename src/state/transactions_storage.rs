use std::cell::RefCell;

use candid::CandidType;
use ic_cdk::api::time;
use ic_stable_structures::{memory_manager::MemoryId, StableBTreeMap, StableCell};
use serde::{Deserialize, Serialize};

use crate::{
    state::{MemoryIndex, MEMORY_MANAGER, VM},
    types::cbor_wrapper::Cbor,
};

#[derive(Debug, Serialize, Deserialize, CandidType, Clone, PartialEq, Eq)]
pub enum TxTypeEnum {
    Stake,
    Unstake,
    Reward,
}

#[derive(Debug, Serialize, Deserialize, CandidType, Clone)]
pub struct TxRecord {
    pub txid: String,
    pub liq_amount: u128,
    pub sliq_amount: u128,
    pub tx_type: TxTypeEnum,

    #[serde(default)]
    pub timestamp: u64,
}

#[derive(CandidType, Serialize, Deserialize, Clone)]
pub struct UnstakeUtxo {
    pub utxo: String,
    pub timestamp: u64,
    pub prev_utxos: Vec<String>,
}

thread_local! {
    /// Storage for stake, unstake and reward transactions
    /// Key: TxId
    /// Value: (liq_amount, sliq_amount)
    pub static TRANSACTION_RECORDS: RefCell<StableBTreeMap<String, Cbor<TxRecord>, VM>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::TransactionsStorageMemoryId as u8))),
        )
    );

    /// Storage for processed tranasctions
    /// Key: TxId
    /// Value: bool
    pub static PROCESSED_TRANSACTIONS: RefCell<StableBTreeMap<String, u64, VM>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::ProcessedTranasctionMemoryId as u8))),
        )
    );

    /// Storage for available token utxos
    /// Key: Utxo
    /// Value: UnstakeUtxo
    pub static AVAILABLE_UNSTAKE_UTXOS: RefCell<StableBTreeMap<String, Cbor<UnstakeUtxo>, VM>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::AvailableUnstakeUtxosMemeoryId as u8))),
        )
    );

    // Tracks the last block from which we performed a scan
    pub static LAST_BLOCK: RefCell<StableCell<Option<u128>, VM>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::ScannedBlockTranasctionMemoryId as u8)))
            , None)
            .expect(concat!("Failed to init stable cell for balance"))
    );
}

// Create
pub fn insert_transaction(tx_record: TxRecord) {
    TRANSACTION_RECORDS.with(|records| {
        records
            .borrow_mut()
            .insert(tx_record.txid.clone(), Cbor(tx_record));
    });
}

// Read
pub fn get_transaction(tx_id: &String) -> Option<TxRecord> {
    TRANSACTION_RECORDS.with(|records| {
        if let Some(record) = records.borrow().get(tx_id) {
            return Some(record.0.clone());
        }
        None
    })
}

// Delete
pub fn delete_transaction(tx_id: &String) {
    TRANSACTION_RECORDS.with(|records| {
        records.borrow_mut().remove(tx_id);
    });
}

// Add
pub fn add_unstake_utxo(key: &str, value: &UnstakeUtxo) {
    AVAILABLE_UNSTAKE_UTXOS.with(|m| m.borrow_mut().insert(key.to_string(), Cbor(value.clone())));
}

// Has
pub fn contains_unstake_utxo(key: &str) -> bool {
    AVAILABLE_UNSTAKE_UTXOS.with(|m| m.borrow().contains_key(&key.to_string()))
}

// Delete
pub fn delete_prev_utxos(current_tx_id: &String) {
    let utxos: Vec<String> = AVAILABLE_UNSTAKE_UTXOS.with_borrow(|records| {
        records
            .iter()
            .filter(|item| item.0.starts_with(current_tx_id))
            .flat_map(|item| item.1 .0.prev_utxos.clone())
            .collect()
    });

    AVAILABLE_UNSTAKE_UTXOS.with_borrow_mut(|records| {
        for utxo in utxos {
            records.remove(&utxo);
        }
    });
}

// Add
pub fn add_processed_tx(key: &str) {
    let time = time() / 1e9 as u64;
    PROCESSED_TRANSACTIONS.with_borrow_mut(|txs| txs.insert(key.to_string(), time));
}
