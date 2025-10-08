use std::collections::HashSet;

use bitcoin::{hashes::Hash, Txid};
use ic_cdk::{api::time, bitcoin_canister::GetUtxosRequest};
use mockall::automock;

#[derive(Default)]
pub struct BitcoinOracle;

#[automock]
#[async_trait::async_trait]
pub trait BitcoinOracleTrait: Send + Sync {
    fn get_time(&self) -> u64;
    async fn get_confirmed_transactions(
        &self,
        address: &str,
        min_confirmation: u8,
        until_block: u128,
    ) -> Result<Vec<(u128, String)>, String>;
}

#[async_trait::async_trait]
impl BitcoinOracleTrait for BitcoinOracle {
    fn get_time(&self) -> u64 {
        time() / 1e9 as u64
    }

    async fn get_confirmed_transactions(
        &self,
        address: &str,
        min_confirmation: u8,
        until_block: u128,
    ) -> Result<Vec<(u128, String)>, String> {
        let res = ic_cdk::bitcoin_canister::bitcoin_get_utxos(&GetUtxosRequest {
            address: address.to_string(),
            filter: Some(ic_cdk::bitcoin_canister::UtxosFilter::MinConfirmations(
                min_confirmation as u32,
            )),
            network: ic_cdk::bitcoin_canister::Network::Mainnet,
        })
        .await
        .map_err(|e| format!("Could not get address utxos {}", e))?;

        let mut tranasactions = HashSet::new();
        for item in res
            .utxos
            .iter()
            .filter(|item| item.height >= until_block as u32)
        {
            let txid = Txid::from_slice(&item.outpoint.txid)
                .map_err(|e| format!("could not decode txid {}", e))?;

            let utxo = format!("{}", txid);
            tranasactions.insert((item.height as u128, utxo));
        }

        Ok(tranasactions.iter().cloned().collect())
    }
}
