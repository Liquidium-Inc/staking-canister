use std::cell::RefCell;

use candid::{CandidType, Principal};

use ic_cdk::api::time;
use serde::Deserialize;

use crate::oracle::mempool::{MempoolClient, MempoolClientTrait};

use super::ord_client::RuneUtxoInfo;

use num_traits::ToPrimitive;

thread_local! {
    static LAST_SYNC: RefCell<u64> = const { RefCell::new(0) };
}

pub struct OmnityOrdClient;
#[derive(CandidType, Deserialize)]
pub enum Error {
    MaxOutpointsExceeded,
}

#[derive(CandidType, Deserialize)]
pub struct RuneBalance {
    pub confirmations: u32,
    pub divisibility: u8,
    pub amount: candid::Nat,
    pub rune_id: String,
    pub symbol: Option<String>,
}

#[derive(CandidType, Deserialize)]
pub enum OmnityResult {
    Ok(Vec<Option<Vec<RuneBalance>>>),
    Err(Error),
}

impl OmnityOrdClient {
    pub async fn get_latest_block() -> Result<u32, String> {
        let indexer = Principal::from_text("kzrva-ziaaa-aaaar-qamyq-cai")
            .map_err(|_| "could not decode omnity canister principal".to_string())?;

        let call_result = ic_cdk::call::Call::bounded_wait(indexer, "get_latest_block")
            .await
            .map_err(|e| format!("Failed to call omonity {}", e))?;

        call_result.candid().map_err(|e| e.to_string())
    }

    pub async fn check_sync(force: bool) -> Result<(), String> {
        let last_sync = LAST_SYNC.with_borrow(|t| *t);

        let now_secs = time() / 1e9 as u64;
        let dt = now_secs.saturating_sub(last_sync);

        // Sync cooldown in seconds
        if !force && dt < 30u64 {
            return Ok(());
        }

        let mempool_client = MempoolClient;
        let (idx_res, node_res) = futures::join!(
            OmnityOrdClient::get_latest_block(),       // -> Result<u64, _>
            mempool_client.get_current_block_height()  // -> Result<u64, _>
        );

        let idx = idx_res.map_err(|e| format!("indexer err: {e}"))?;
        let node = node_res.map_err(|e| format!("node err: {e}"))?;

        let delta = idx.abs_diff(node);
        if delta > 0 {
            return Err(format!(
                "Indexer out of sync (indexer={idx}, node={node}, delta={delta})"
            ));
        }

        LAST_SYNC.set(now_secs);
        Ok(())
    }

    pub async fn batched_get_rune_utxo_info(
        utxos: &Vec<String>,
    ) -> Result<Vec<Option<RuneUtxoInfo>>, String> {
        let indexer = Principal::from_text("kzrva-ziaaa-aaaar-qamyq-cai")
            .map_err(|_| "could not decode omnity canister principal".to_string())?;

        // Make sure that omnity is in sync
        OmnityOrdClient::check_sync(false).await?;

        let call_result =
            ic_cdk::call::Call::bounded_wait(indexer, "get_rune_balances_for_outputs")
                .with_arg(utxos)
                .await
                .map_err(|e| format!("Failed to call omonity {}", e))?;

        let call_result = call_result.candid().expect("Could not fetch utxo info");
        let call_result = match call_result {
            OmnityResult::Ok(res) => {
                let result: Vec<Option<RuneUtxoInfo>> = res
                    .iter()
                    .map(|item| {
                        if let Some(item) = item {
                            let mut utxo_info = RuneUtxoInfo {
                                decimals: vec![],
                                rune_balances: vec![],
                                rune_ids: vec![],
                            };
                            for balance in item {
                                if let Some(balance_u128) = balance.amount.0.to_u128() {
                                    utxo_info.decimals.push(balance.divisibility);

                                    utxo_info.rune_balances.push(balance_u128);
                                    utxo_info.rune_ids.push(balance.rune_id.clone());
                                }
                            }

                            Some(utxo_info)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<Option<RuneUtxoInfo>>>();

                Ok(result)
            }
            OmnityResult::Err(_) => Err("Omnity Error: Could not decode utxo info".to_string()),
        };

        call_result
    }
}
