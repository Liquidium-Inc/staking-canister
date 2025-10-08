use std::time::Duration;

use ic_canister_log::log;
use ic_cdk::api::time;

use crate::{
    config::default_config,
    get_exchange_rate_components, get_pool_address,
    log::{ERR, LOG},
    oracle::{
        bitcoin::{BitcoinOracle, BitcoinOracleTrait},
        mempool::{MempoolClient, MempoolClientTrait, Transaction, TxStatus},
        ord_client::OrdClientTrait,
    },
    state::{
        add_pending_rewards, apply_rewards, get_ord_client, set_exchange_rate_components,
        transactions_storage::{
            self, delete_prev_utxos, delete_transaction, get_transaction, insert_transaction,
            TxRecord, TxTypeEnum, UnstakeUtxo, LAST_BLOCK, PROCESSED_TRANSACTIONS,
            TRANSACTION_RECORDS,
        },
    },
};

pub struct TransactionChecker<C: MempoolClientTrait, O: OrdClientTrait, B: BitcoinOracleTrait> {
    pub mempool_client: C,
    pub ord_client: O,
    pub bitcoin_client: B,
}

// Transactions that are older than this are removed
pub const TX_CONFIRMATION_TIMEOUT: u64 = 3600 * 2; // 2 hours

// Processed txs that are older than 30 days are removed
pub const PROCESSED_TRANSACTIONS_TIMEOUT: u64 = 3600 * 24 * 30;

pub fn init() {
    ic_cdk_timers::set_timer_interval(Duration::from_secs(300), || {
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
                    log!(ERR, "{} ", e);
                });
        })
    });

    ic_cdk_timers::set_timer_interval(Duration::from_secs(300), || {
        ic_cdk::futures::spawn(async {
            let ord_client = get_ord_client();
            let tx_checker = TransactionChecker {
                mempool_client: MempoolClient,
                ord_client,
                bitcoin_client: BitcoinOracle,
            };

            let _ = tx_checker
                .process_transaction_records()
                .await
                .inspect_err(|e| {
                    log!(ERR, "{} ", e);
                });
        });
    });

    ic_cdk_timers::set_timer_interval(Duration::from_secs(300), || {
        ic_cdk::futures::spawn(async {
            let ord_client = get_ord_client();
            let tx_checker = TransactionChecker {
                mempool_client: MempoolClient,
                ord_client,
                bitcoin_client: BitcoinOracle,
            };

            let _ = tx_checker.cleanup_transaction_records().inspect_err(|e| {
                log!(ERR, "{} ", e);
            });
        });
    });

    // Remove processed transactions every 24 hours
    ic_cdk_timers::set_timer_interval(Duration::from_secs(3600 * 24), || {
        ic_cdk::futures::spawn(async {
            let ord_client = get_ord_client();
            let tx_checker = TransactionChecker {
                mempool_client: MempoolClient,
                ord_client,
                bitcoin_client: BitcoinOracle,
            };

            let _ = tx_checker
                .cleanup_processed_transaction()
                .await
                .inspect_err(|e| {
                    log!(ERR, "{} ", e);
                });
        });
    });

    //
    ic_cdk_timers::set_timer_interval(Duration::from_secs(3600 * 24), || {
        ic_cdk::futures::spawn(async {
            let ord_client = get_ord_client();
            let tx_checker = TransactionChecker {
                mempool_client: MempoolClient,
                ord_client,
                bitcoin_client: BitcoinOracle,
            };

            let _ = tx_checker
                .cleanup_processed_transaction()
                .await
                .inspect_err(|e| {
                    log!(ERR, "{} ", e);
                });
        });
    });
}

impl<C: MempoolClientTrait, O: OrdClientTrait, B: BitcoinOracleTrait> TransactionChecker<C, O, B> {
    /**
     * Checks for new reward transactions.
     * Reward transactions don't have any inputs coming from the canister
     * and are sending liq tokens to the canister address
     */
    pub async fn scan_for_new_reward_transactions(&self) -> Result<(), String> {
        let address = get_pool_address(None);
        let mut last_block = LAST_BLOCK.with_borrow(|block| *block.get());

        if last_block.is_none() {
            let latest_block = self
                .mempool_client
                .get_current_block_height()
                .await
                .map_err(|e| e.to_string())?;
            last_block = Some(latest_block as u128 - 2000);
        }

        let confirmed_transactions = self
            .bitcoin_client
            .get_confirmed_transactions(&address, 4, last_block.unwrap())
            .await
            .map_err(|e| format!("Failed to fetch pool transaction {}", e))?;

        for txid in confirmed_transactions
            .iter()
            .map(|item| item.1.clone())
            .collect::<Vec<_>>()
        {
            let exists =
                PROCESSED_TRANSACTIONS.with_borrow(|processed| processed.contains_key(&txid));

            if exists {
                continue;
            }

            let tx = match self.mempool_client.get_transaction_info(txid).await {
                Ok(tx) => tx,
                Err(e) => {
                    ic_cdk::eprintln!("{}", e.to_string());
                    continue;
                }
            };

            // Check if it is a reward transaction
            let has_canister_input = tx
                .vin
                .iter()
                .any(|item| item.prevout.scriptpubkey_address == address.clone());

            if has_canister_input {
                // Not a reward transaction because it has a canister input, mark tx as processed
                transactions_storage::add_processed_tx(&tx.txid);
                continue;
            }

            let rune_id = default_config().bitcoin.liq_rune_id;

            // Now we need to make sure that the liq tokens are being sent to the canister address
            let (received_amount, rune_bearing_utxos) =
                self.is_receving_runes(&rune_id, &address, &tx).await?;

            if received_amount == 0 {
                continue;
            }

            let timestamp = self.bitcoin_client.get_time();

            // And if we do receive tokens we need to schedule a tx record
            insert_transaction(TxRecord {
                txid: tx.txid.clone(),
                liq_amount: received_amount,
                sliq_amount: 0u128,
                tx_type: TxTypeEnum::Reward,
                timestamp,
            });

            let time = time() / 1e9 as u64;
            for utxo in &rune_bearing_utxos {
                transactions_storage::add_unstake_utxo(
                    utxo,
                    &UnstakeUtxo {
                        timestamp: time,
                        utxo: utxo.to_string(),
                        prev_utxos: vec![],
                    },
                );
            }

            // Mark tx as processed
            transactions_storage::add_processed_tx(&tx.txid);
        }

        let current_block = self.mempool_client.get_current_block_height().await?;
        let _ = LAST_BLOCK.with_borrow_mut(|b| b.set(Some((current_block - 6).into())));

        Ok(())
    }

    /**
     * Returns the amount of a rune that a address will receive and the rune bearing
     * from a specific tx
     */
    pub async fn is_receving_runes(
        &self,
        rune_id: &String,
        receiver: &String,
        tx: &Transaction,
    ) -> Result<(u128, Vec<String>), String> {
        // Filter out any utxo not beloning to our receiver
        let utxos_to_scan = tx
            .vout
            .iter()
            .enumerate()
            .map(|(index, item)| {
                (
                    format!("{}:{}", tx.txid, index),
                    item.scriptpubkey_address.clone(),
                )
            })
            .filter(|(_, address)| address == receiver)
            .map(|item| item.0)
            .collect::<Vec<String>>();

        let utxo_map = self
            .ord_client
            .get_rune_utxo_info_map(&utxos_to_scan)
            .await?;

        // Get the rune amount that each utxo holds and calculate the total
        let (total_rune_amount, _) = utxo_map.values().fold((0u128, 0u8), |acc, e| {
            if let Some(e) = e {
                match e.rune_ids.iter().position(|p| p == rune_id) {
                    Some(position) => (acc.0 + e.rune_balances[position], e.decimals[position]),
                    None => acc,
                }
            } else {
                acc
            }
        });

        let rune_bearning_utxos = utxo_map
            .iter()
            .filter(|item| item.1.is_some())
            .map(|item| item.0)
            .cloned()
            .collect();

        Ok((total_rune_amount, rune_bearning_utxos))
    }

    /**
     * Loops over our transaction records and removes stale ones
     */
    pub fn cleanup_transaction_records(&self) -> Result<(), String> {
        let time = time() / 1e9 as u64;
        let txs: Vec<(String, TxRecord)> = TRANSACTION_RECORDS.with_borrow(|transactions| {
            transactions
                .iter()
                .map(|item| (item.0, item.1 .0))
                .collect()
        });

        for tx in txs {
            if time - tx.1.timestamp > TX_CONFIRMATION_TIMEOUT {
                TRANSACTION_RECORDS.with_borrow_mut(|transactions| transactions.remove(&tx.0));
            }
        }

        // Apply rewards
        apply_rewards();

        Ok(())
    }

    /**
     * Loops over our processed transaction records and removes old ones
     */
    pub async fn cleanup_processed_transaction(&self) -> Result<(), String> {
        let time = time() / 1e9 as u64;
        let txs: Vec<(String, u64)> = PROCESSED_TRANSACTIONS
            .with_borrow(|transactions| transactions.iter().map(|item| (item.0, item.1)).collect());

        for tx in txs {
            if time - tx.1 > PROCESSED_TRANSACTIONS_TIMEOUT {
                PROCESSED_TRANSACTIONS.with_borrow_mut(|transactions| transactions.remove(&tx.0));
            }
        }

        Ok(())
    }

    /**
     * Loops over our transaction records and updates the
     * exchange rate after 6 confirmations
     */
    pub async fn process_transaction_records(&self) -> Result<(), String> {
        let last_block = self.mempool_client.get_current_block_height().await?;
        let current_block = last_block + 1;
        let mut tx_requests = vec![];
        TRANSACTION_RECORDS.with_borrow(|transactions| {
            let iterator = transactions.iter().take(100);
            for tx in iterator {
                let tx_status = self.mempool_client.get_transaction_status(tx.0);
                tx_requests.push(tx_status);
            }
        });

        let result = futures::future::join_all(tx_requests).await;

        for tx_status_result in result {
            match tx_status_result {
                Ok(tx_status) => self.handle_confirmed_tx(tx_status, current_block as u64),
                Err(e) => {
                    ic_cdk::println!("Tx status error {}", e.to_string())
                }
            }
        }

        Ok(())
    }

    fn handle_confirmed_tx(&self, tx_status: TxStatus, current_block: u64) {
        const CONFIRMATIONS: u64 = 4;
        match tx_status {
            TxStatus::Confirmed { block_height, txid } => {
                if current_block - block_height >= CONFIRMATIONS {
                    let tx_record = get_transaction(&txid).expect("txid does not exist");

                    let rate_components = get_exchange_rate_components().unwrap_or((None, None));
                    let circulating_sliq = rate_components.0.unwrap_or(0u128);
                    let liq_supply = rate_components.1.unwrap_or(0u128);

                    // Update the exchange rate components baed on tx type
                    match tx_record.tx_type {
                        TxTypeEnum::Stake => {
                            log!(
                                LOG,
                                "[Stake] {} -> {} + {} LIQ ,{} -> {} SLIQ {} +{}  ",
                                liq_supply,
                                liq_supply + tx_record.liq_amount,
                                tx_record.liq_amount,
                                circulating_sliq,
                                circulating_sliq + tx_record.sliq_amount,
                                tx_record.sliq_amount,
                                txid
                            );

                            set_exchange_rate_components(
                                circulating_sliq + tx_record.sliq_amount,
                                liq_supply + tx_record.liq_amount,
                            );
                        }
                        TxTypeEnum::Unstake => {
                            log!(
                                LOG,
                                "[Unstake] {} -> {} + {} LIQ ,{} -> {} SLIQ {} +{}  ",
                                liq_supply,
                                liq_supply - tx_record.liq_amount,
                                tx_record.liq_amount,
                                circulating_sliq,
                                circulating_sliq - tx_record.sliq_amount,
                                tx_record.sliq_amount,
                                txid
                            );

                            set_exchange_rate_components(
                                circulating_sliq - tx_record.sliq_amount,
                                liq_supply - tx_record.liq_amount,
                            );
                        }
                        TxTypeEnum::Reward => {
                            add_pending_rewards(tx_record.liq_amount);
                        }
                    }

                    // Remove transaction & any related utxos after confirmation
                    delete_transaction(&txid);
                    delete_prev_utxos(&txid);
                }
            }
            _ => {}
        }
    }
}
