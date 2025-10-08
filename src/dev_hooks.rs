use crate::core::transaction_parser::TransactionChecker;
use crate::oracle::bitcoin::{BitcoinOracle, BitcoinOracleTrait};
use crate::oracle::mempool::MempoolClient;
use crate::oracle::omnity::OmnityOrdClient;
use crate::state::transactions_storage::{
    TxRecord, UnstakeUtxo, AVAILABLE_UNSTAKE_UTXOS, PROCESSED_TRANSACTIONS, TRANSACTION_RECORDS,
};
use crate::state::unstake_storage::UNSTAKE_RECORDS;
use crate::state::{get_ord_client, get_pool_address, BALANCE, CIRCULATING_SUPPLY};
use crate::types::cbor_wrapper::Cbor;

use ic_cdk::api::time;
use ic_cdk::update;

#[update]
pub async fn debug_confimed_tx_fetcher(min: u8, max: u128) -> Result<Vec<(u128, String)>, String> {
    let oracle = BitcoinOracle;
    let pool_address = get_pool_address(0);
    oracle
        .get_confirmed_transactions(&pool_address, min, max)
        .await
}

#[update]
pub fn remove_processing(txid: String) {
    PROCESSED_TRANSACTIONS.with_borrow_mut(|pt| pt.remove(&txid));
}

#[update]
pub fn remove_recorded(txid: String) {
    TRANSACTION_RECORDS.with_borrow_mut(|pt| pt.remove(&txid));
}

#[update]
pub fn update_unstake_record_utxo(utxo: String, new_utxo: String) -> Result<(), String> {
    UNSTAKE_RECORDS.with_borrow_mut(|pt| {
        if let Some(record) = pt.iter().find(|item| item.1 .0.utxo == utxo) {
            let mut data = record.1 .0.clone();
            data.utxo = new_utxo;
            pt.insert(record.0, Cbor(data));
        }
    });

    Ok(())
}

#[update]
pub fn remove_all_processing() {
    PROCESSED_TRANSACTIONS.with_borrow_mut(|pt| pt.clear_new());
}

#[update]
pub fn remove_all_tx_records() {
    TRANSACTION_RECORDS.with_borrow_mut(|pt| pt.clear_new());
}

#[update]
pub fn insert_record(tx_record: TxRecord) {
    TRANSACTION_RECORDS
        .with_borrow_mut(|pt| pt.insert(tx_record.txid.clone(), Cbor(tx_record.clone())));
}

#[update]
pub fn remove_record(txid: String) {
    TRANSACTION_RECORDS.with_borrow_mut(|pt| pt.remove(&txid));
}

#[update]
pub fn reset_exchange_rate() {
    CIRCULATING_SUPPLY.with_borrow_mut(|c| c.set(None).ok());
    BALANCE.with_borrow_mut(|b| b.set(None).ok());
}

#[update]
pub fn add_unstake_utxos(utxos: Vec<String>) {
    let time = time() / 1e9 as u64;
    AVAILABLE_UNSTAKE_UTXOS.with_borrow_mut(|records| {
        for utxo in &utxos {
            records.insert(
                utxo.clone(),
                Cbor(UnstakeUtxo {
                    timestamp: time,
                    prev_utxos: vec![],
                    utxo: utxo.to_string(),
                }),
            );
        }
    })
}

#[update]
pub fn clean_unstake_utxos() {
    AVAILABLE_UNSTAKE_UTXOS.with_borrow_mut(|records| {
        records.clear_new();
    });
}

#[update]
pub async fn get_omnity_block() -> Result<u32, String> {
    OmnityOrdClient::get_latest_block().await
}

#[update]
pub fn start_cron() {
    ic_cdk::futures::spawn(async {
        let ord_client = get_ord_client();
        let tx_checker = TransactionChecker {
            mempool_client: MempoolClient,
            ord_client,
            bitcoin_client: BitcoinOracle,
        };

        let _ = tx_checker
            .scan_for_new_reward_transactions()
            .await
            .inspect_err(|e| {
                ic_cdk::println!("Error {e}");
            });

        let _ = tx_checker
            .process_transaction_records()
            .await
            .inspect_err(|e| {
                ic_cdk::println!("Error {e}");
            });
    });
}

/// Debug | Parses a PSBT and returns the runes data and debug information as JSON
#[update]
pub fn parse_psbt_runes(input_psbt_base64: String) -> String {
    match ordinals_runes::parse_psbt_runes_with_debug(&input_psbt_base64) {
        Ok((rune_data, debug_output)) => {
            // Create a combined result with both the rune data and debug info
            let mut combined_result = serde_json::Map::new();

            // Add the rune data
            match serde_json::to_value(&rune_data) {
                Ok(rune_value) => {
                    combined_result.insert("rune_data".to_string(), rune_value);
                }
                Err(e) => {
                    return format!("Error serializing rune data: {}", e);
                }
            }

            // Add the debug output as a string
            combined_result.insert(
                "debug_output".to_string(),
                serde_json::Value::String(debug_output),
            );

            // Serialize the combined result
            match serde_json::to_string_pretty(&combined_result) {
                Ok(json) => json,
                Err(e) => format!("Error serializing combined result: {}", e),
            }
        }
        Err(e) => format!("Error parsing PSBT: {}", e),
    }
}
