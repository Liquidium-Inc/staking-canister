use ic_cdk::management_canister::HttpHeader;
use mockall::automock;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    oracle::{
        http::{http_get_call, HttpError},
        ord_client::from_name,
    },
    state::get_mempool_url,
};

#[derive(Default, Debug)]
pub enum TxStatus {
    Confirmed {
        block_height: u64,
        txid: String,
    },
    #[default]
    Unconfirmed,
    NotFound,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Transaction {
    pub txid: String,
    pub version: u32,
    pub locktime: u32,
    pub vin: Vec<Vin>,
    pub vout: Vec<Vout>,
    pub size: u32,
    pub weight: u32,
    pub sigops: u32,
    pub fee: u64,
    pub status: Status,
}

#[cfg(test)]
impl Into<bitcoin::Transaction> for Transaction {
    fn into(self) -> bitcoin::Transaction {
        use std::str::FromStr;

        use bitcoin::{
            absolute::LockTime, OutPoint, ScriptBuf, Sequence, Transaction as BtcTransaction, TxIn,
            TxOut, Witness,
        };

        let inputs = self
            .vin
            .into_iter()
            .map(|vin| TxIn {
                previous_output: OutPoint {
                    txid: bitcoin::Txid::from_str(&vin.txid).unwrap(),
                    vout: vin.vout,
                },
                script_sig: ScriptBuf::from(hex::decode(vin.scriptsig).unwrap()),
                sequence: Sequence(vin.sequence),
                witness: Witness::default(),
            })
            .collect();

        let outputs = self
            .vout
            .into_iter()
            .map(|vout| TxOut {
                value: vout.value,
                script_pubkey: ScriptBuf::from(hex::decode(vout.scriptpubkey).unwrap()),
            })
            .collect();

        BtcTransaction {
            version: self.version as i32,
            lock_time: LockTime::ZERO,
            input: inputs,
            output: outputs,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct Status {
    pub confirmed: bool,
    pub block_height: Option<u64>,
    pub block_hash: Option<String>,
    pub block_time: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Vin {
    pub txid: String,
    pub vout: u32,
    pub prevout: Prevout,
    pub scriptsig: String,
    pub scriptsig_asm: String,
    pub witness: Vec<String>,
    pub is_coinbase: bool,
    pub sequence: u32,
    pub inner_witnessscript_asm: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Prevout {
    pub scriptpubkey: String,
    pub scriptpubkey_asm: String,
    pub scriptpubkey_type: String,
    #[serde(default)]
    pub scriptpubkey_address: String,
    pub value: u64,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct Vout {
    pub scriptpubkey: String,
    pub scriptpubkey_asm: String,
    pub scriptpubkey_type: String,
    #[serde(default)]
    pub scriptpubkey_address: String,
    pub value: u64,
}

#[derive(Debug, Error)]
#[error("Tx status error : {0}")]
pub struct TxStatusError(String);

#[automock]
#[async_trait::async_trait]
pub trait MempoolClientTrait: Send + Sync {
    #[allow(dead_code)]
    async fn get_transaction_hex(&self, txid: &str) -> Result<String, HttpError>;
    async fn get_current_block_height(&self) -> Result<u32, String>;
    async fn get_transaction_status(&self, txid: String) -> Result<TxStatus, TxStatusError>;
    async fn get_transaction_info(&self, txid: String) -> Result<Transaction, HttpError>;
}

pub struct MempoolClient;

#[async_trait::async_trait]
impl MempoolClientTrait for MempoolClient {
    async fn get_current_block_height(&self) -> Result<u32, String> {
        let mempool_url = get_mempool_url().expect("could not get mempool url");
        let url = format!("{}/api/blocks/tip/height", mempool_url);
        let response = http_get_call(
            url,
            vec![
                HttpHeader {
                    name: "Content-Type".to_string(),
                    value: "application/json".to_string(),
                },
                HttpHeader {
                    name: "Accept".to_string(),
                    value: "application/json".to_string(),
                },
            ],
            Some(from_name("transform_ord_response".to_string(), vec![])),
        )
        .await
        .map_err(|e| e.to_string())?;
        let res = response.as_u64().unwrap();
        Ok(res as u32)
    }

    async fn get_transaction_status(&self, txid: String) -> Result<TxStatus, TxStatusError> {
        let response = self.get_transaction_info(txid.clone()).await;

        match response {
            Ok(result) => {
                if result.status.confirmed {
                    Ok(TxStatus::Confirmed {
                        block_height: result.status.block_height.unwrap(),
                        txid,
                    })
                } else {
                    Ok(TxStatus::Unconfirmed)
                }
            }
            Err(err) => match err {
                HttpError::NotFound => Ok(TxStatus::NotFound),
                _ => Err(TxStatusError(err.to_string())),
            },
        }
    }

    async fn get_transaction_info(&self, txid: String) -> Result<Transaction, HttpError> {
        let mempool_url = get_mempool_url().expect("could not get mempool url");
        let url = format!("{}/api/tx/{}", mempool_url, txid);
        let response = http_get_call(
            url,
            vec![
                HttpHeader {
                    name: "Content-Type".to_string(),
                    value: "application/json".to_string(),
                },
                HttpHeader {
                    name: "Accept".to_string(),
                    value: "application/json".to_string(),
                },
            ],
            Some(from_name("transform_ord_response".to_string(), vec![])),
        )
        .await?;
        serde_json::from_value(response)
            .map_err(|e| HttpError::Generic(format!("Deserialization error {}", e)))
    }

    #[allow(unused)]
    async fn get_transaction_hex(&self, txid: &str) -> Result<String, HttpError> {
        let mempool_url = get_mempool_url().expect("could not get mempool url");
        let url = format!("{}/api/tx/{}/hex", mempool_url, txid);

        let response = http_get_call(
            url,
            vec![
                HttpHeader {
                    name: "Content-Type".to_string(),
                    value: "application/json".to_string(),
                },
                HttpHeader {
                    name: "Accept".to_string(),
                    value: "application/json".to_string(),
                },
            ],
            Some(from_name("transform_raw_response".to_string(), vec![])),
        )
        .await?;

        response["hex"]
            .as_str().map(|val| val.to_string())
            .ok_or(HttpError::Generic("Could not decode hex".to_string()))
    }
}
