//! State management for the Liquidium staking canister.

use std::cell::{Cell, RefCell};

use bitcoin::Network;
use candid::Principal;
use ic_canister_log::log;
use ic_cdk::api::msg_caller;
use ic_cdk::bitcoin_canister::Network as BitcoinNetwork;
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::{DefaultMemoryImpl, StableBTreeMap, StableCell};

use crate::log::LOG;
use crate::oracle::ord_client::OrdClient;
use crate::state::transactions_storage::{TxTypeEnum, TRANSACTION_RECORDS};
use crate::{config, get_exchange_rate_components};

pub mod transactions_storage;
pub mod unstake_storage;

pub use unstake_storage::{
    get_all_unstake_records, get_latest_unstake_record, get_user_unstake_records,
    store_unstake_record, UnstakeRecord,
};

pub enum MemoryIndex {
    PoolAddressMemoryId = 0,
    XpubAddressMemoryId = 1,
    FingerprintMemoryId = 2,
    CirculatingSupplyMemoryId = 3,
    BalanceMemoryId = 4,
    UnstakeRecordsMemoryId = 5,
    UnstakeStorageCounterMemoryId = 6,
    TransactionsStorageMemoryId = 7,
    MempoolUrlMemoryId = 8,
    ProcessedTranasctionMemoryId = 9,
    ScannedBlockTranasctionMemoryId = 10,
    AdminsMemoryId = 11,
    AvailableUnstakeUtxosMemeoryId = 12,
    PendingRewardsMemeoryId = 13,
}

type VM = VirtualMemory<DefaultMemoryImpl>;

thread_local! {
    // The memory manager is used for simulating multiple memories. Given a `MemoryId` it can
    // return a memory that can be used by stable structures.
    pub static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));


    // Bitcoin network setting
    static NETWORK: Cell<BitcoinNetwork> = const { Cell::new(BitcoinNetwork::Mainnet) };

    // ECDSA key name
    static KEY_NAME: RefCell<String> = RefCell::new(String::from("key_1"));

    // Pool addresses by index
    static POOL_ADDRESSES: RefCell<StableBTreeMap<u32, String, VM>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::PoolAddressMemoryId as u8))),
        )
    );

    // Pool xpubs by index
    static POOL_XPUBS: RefCell<StableBTreeMap<u32, String, VM>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::XpubAddressMemoryId as u8))),
        )
    );

    // Pool fingerprints by index
    static POOL_FINGERPRINTS: RefCell<StableBTreeMap<u32, String, VM>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::FingerprintMemoryId as u8))),
        )
    );

    // Exchange rate components storage
    pub static CIRCULATING_SUPPLY: RefCell<StableCell<Option<u128>, VM>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::CirculatingSupplyMemoryId as u8)))
            , None)
            .expect(concat!("Failed to init stable cell for circulating supply"))
        );

    pub static BALANCE: RefCell<StableCell<Option<u128>, VM>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::BalanceMemoryId as u8)))
            , None)
            .expect(concat!("Failed to init stable cell for balance"))
    );

    // Mempoolurl
    static MEMPOOL_URL:RefCell<StableCell<Option<String>, VM>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::MempoolUrlMemoryId as u8)))
            , Some("https://mempool.space".to_string()))
            .expect(concat!("Failed to init stable cell for balance"))
    );

    // List of principals that are allowed to call the canister
    pub static ALLOWED_CALLERS: RefCell<StableBTreeMap<Principal, bool, VM>> = RefCell::new(
            StableBTreeMap::init(
                MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::AdminsMemoryId as u8))),
            )
    );

      // Pending rewards to be applied
    pub static PENDING_REWARDS: RefCell<StableCell<Option<u128>, VM>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with(|m| m.borrow().get(MemoryId::new(MemoryIndex::PendingRewardsMemeoryId as u8)))
            , None)
            .expect(concat!("Failed to init stable cell for circulating supply"))
        );


}

/// Initializes canister state
pub fn init() {
    let default_config = config::default_config();
    NETWORK.with(|n| n.set(default_config.bitcoin.network));
}

/// Returns current Bitcoin network
pub fn get_network() -> BitcoinNetwork {
    NETWORK.with(|n| n.get())
}

/// Returns ECDSA key name
pub fn get_key_name() -> String {
    KEY_NAME.with_borrow(|k| k.clone())
}

/// Sets pool address for index
pub fn set_pool_address(address: String, index: u32) {
    POOL_ADDRESSES.with(|addresses| {
        addresses.borrow_mut().insert(index, address);
    });
}

/// Returns pool address for index
pub fn get_pool_address(index: u32) -> String {
    POOL_ADDRESSES.with_borrow(|addresses| addresses.get(&index).clone().unwrap_or_default())
}

/// Sets pool xpub for index
pub fn set_pool_xpub(xpub: String, index: u32) {
    POOL_XPUBS.with(|xpubs| {
        xpubs.borrow_mut().insert(index, xpub);
    });
}

/// Returns pool xpub for index
pub fn get_pool_xpub(index: u32) -> String {
    POOL_XPUBS.with(|xpubs| xpubs.borrow().get(&index).clone().unwrap_or_default())
}

/// Sets pool fingerprint for index
pub fn set_pool_fingerprint(fingerprint: String, index: u32) {
    POOL_FINGERPRINTS.with(|fingerprints| {
        fingerprints.borrow_mut().insert(index, fingerprint);
    });
}

/// Returns pool fingerprint for index
pub fn get_pool_fingerprint(index: u32) -> String {
    POOL_FINGERPRINTS.with(|fingerprints| {
        fingerprints
            .borrow()
            .get(&index)
            .clone()
            .unwrap_or_default()
    })
}

/// Checks if pool data exists for index
pub fn has_pool_data(index: u32) -> bool {
    POOL_ADDRESSES.with(|addresses| addresses.borrow().contains_key(&index))
        && POOL_XPUBS.with(|xpubs| xpubs.borrow().contains_key(&index))
        && POOL_FINGERPRINTS.with(|fingerprints| fingerprints.borrow().contains_key(&index))
}

/// Converts to bitcoin::Network type
pub fn get_btc_network() -> Network {
    config::to_bitcoin_network(get_network())
}

// Adds pending rewards
pub fn add_pending_rewards(reward: u128) {
    PENDING_REWARDS.with_borrow_mut(|c| {
        let current = c.get().unwrap_or(0u128);

        log!(
            LOG,
            "[Pending Reward] {} -> {} +{} LIQ ",
            current,
            current + reward,
            reward
        );

        c.set(Some(current + reward)).ok()
    });
}

/// Stores the exchange rate components in local storage
pub fn set_exchange_rate_components(circulating_supply: u128, balance: u128) {
    CIRCULATING_SUPPLY.with_borrow_mut(|c| c.set(Some(circulating_supply)).ok());
    BALANCE.with_borrow_mut(|b| b.set(Some(balance)).ok());
}

/// Initializes genesis exchange-rate components for a fresh pool.
pub fn init_genesis_exchange_rate_components() {
    match (get_stored_circulating_supply(), get_stored_balance()) {
        (None, None) => set_exchange_rate_components(0, 0),
        (Some(_), Some(_)) => {}
        (None, Some(_)) | (Some(_), None) => {
            panic!("Inconsistent exchange rate state detected; manual intervention required");
        }
    }
}

/// Retrieves the stored circulating supply
pub fn get_stored_circulating_supply() -> Option<u128> {
    CIRCULATING_SUPPLY.with(|c| *c.borrow().get())
}

/// Retrieves the stored balance
pub fn get_stored_balance() -> Option<u128> {
    BALANCE.with(|b| *b.borrow().get())
}

/// Calculates and retrieves the exchange rate from stored components
pub fn get_stored_exchange_rate() -> Option<f64> {
    match (get_stored_circulating_supply(), get_stored_balance()) {
        (Some(circulating), Some(balance)) => {
            if circulating > 0 {
                Some(balance as f64 / circulating as f64)
            } else {
                Some(1.0)
            }
        }
        _ => None,
    }
}

/// Get an ord client instance
pub fn get_ord_client() -> OrdClient {
    OrdClient
}

/// Get mempool url
pub fn get_mempool_url() -> Option<String> {
    MEMPOOL_URL.with_borrow(|url| url.get().clone())
}

/// Set mempool url
pub fn set_mempool_url(new_url: Option<String>) {
    MEMPOOL_URL.with_borrow_mut(|url| url.set(new_url).ok());
}

// Checks if the caller is allowed to call the canister
pub async fn allowed() -> Result<(), String> {
    let caller_id = msg_caller();

    ALLOWED_CALLERS.with_borrow(|admins| {
        // Check if the caller is in the list of controllers
        if !admins.contains_key(&caller_id) {
            return Err(format!("Unauthorized access by caller: {}", caller_id));
        }

        Ok(())
    })
}

pub fn apply_rewards() {
    // Apply rewards if processing queue is empty
    TRANSACTION_RECORDS.with_borrow(|records| {
        if records.is_empty() || records.iter().all(|item| item.1.0.tx_type == TxTypeEnum::Reward) {
            PENDING_REWARDS.with_borrow_mut(|rewards| {
                if let Some(r) = rewards.get() {
                    let rate_components = get_exchange_rate_components().unwrap_or((None, None));
                    let circulating_sliq = rate_components.0.unwrap_or(0u128);
                    let liq_supply = rate_components.1.unwrap_or(0u128);

                    log!(
                        LOG,
                        "[Apply Reward] {} -> {} +{} LIQ ",
                        liq_supply,
                        liq_supply + r,
                        r
                    );

                    set_exchange_rate_components(circulating_sliq, liq_supply + r);

                    let _ = rewards.set(None);
                }
            })
        }
    })
}
