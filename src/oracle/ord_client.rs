use std::collections::HashMap;

use ic_cdk::{
    api::canister_self,
    management_canister::{HttpRequestResult, TransformArgs, TransformContext, TransformFunc},
    query,
};
use mockall::automock;

use crate::oracle::omnity::OmnityOrdClient;

#[derive(Debug, Clone)]
pub struct RuneUtxoInfo {
    pub rune_ids: Vec<String>,
    pub rune_balances: Vec<u128>,
    pub decimals: Vec<u8>,
}

pub type RuneUtxoInfoMap = HashMap<String, Option<RuneUtxoInfo>>;

pub struct OrdClient;

// Strips all data that is not needed from the original response.
#[query(hidden = "true")]
fn transform_ord_response(raw: TransformArgs) -> HttpRequestResult {
    let response = raw.response;

    HttpRequestResult {
        headers: vec![],
        status: response.status,
        body: response.body,
    }
}

// Strips all data that is not needed from the original response.
#[query(hidden = "true")]
fn transform_raw_response(raw: TransformArgs) -> HttpRequestResult {
    let response = raw.response;
    let json_body = serde_json::json!({
        "hex": String::from_utf8(response.body).unwrap()
    });

    let body = serde_json::to_vec(&json_body).expect("Failed to serialize JSON");
    HttpRequestResult {
        headers: vec![],
        status: response.status,
        body,
    }
}

/// Constructs a TransformContext from a name and context. The principal is assumed to be the [current canister's](id).
pub fn from_name(candid_function_name: String, context: Vec<u8>) -> TransformContext {
    TransformContext {
        context,
        function: TransformFunc(candid::Func {
            method: candid_function_name,
            principal: canister_self(),
        }),
    }
}

#[automock]
#[async_trait::async_trait]
pub trait OrdClientTrait: Send + Sync {
    async fn get_rune_utxo_info_map(&self, utxos: &Vec<String>) -> Result<RuneUtxoInfoMap, String>;
    async fn get_rune_sent_amount(
        &self,
        utxos: &Vec<String>,
        rune_id: &str,
    ) -> Result<(u128, u8), String>;
}

#[async_trait::async_trait]
impl OrdClientTrait for OrdClient {
    async fn get_rune_utxo_info_map(&self, utxos: &Vec<String>) -> Result<RuneUtxoInfoMap, String> {
        let utxo_info_map: RuneUtxoInfoMap = OmnityOrdClient::batched_get_rune_utxo_info(utxos)
            .await
            .expect("Could not fetch utxo info")
            .iter()
            .enumerate()
            .map(|item| {
                if item.1.is_some() {
                    (utxos.get(item.0).unwrap().clone(), item.1.clone())
                } else {
                    (utxos.get(item.0).unwrap().clone(), None)
                }
            })
            .collect();

        Ok(utxo_info_map)
    }

    async fn get_rune_sent_amount(
        &self,
        utxos: &Vec<String>,
        rune_id: &str,
    ) -> Result<(u128, u8), String> {
        // Fetch the utxo info
        let utxo_info: Vec<RuneUtxoInfo> = OmnityOrdClient::batched_get_rune_utxo_info(utxos)
            .await
            .expect("Could not fetch utxo info")
            .iter()
            .filter_map(|item| if item.is_some() { item.clone() } else { None })
            .collect();

        // Get the rune amount that each utxo holds and calculate the total
        let (total_rune_amount, decimals) = utxo_info.iter().fold((0u128, 0u8), |acc, e| {
            match e.rune_ids.iter().position(|p| p == rune_id) {
                Some(position) => (acc.0 + e.rune_balances[position], e.decimals[position]),
                None => acc,
            }
        });

        Ok((total_rune_amount, decimals))
    }
}
