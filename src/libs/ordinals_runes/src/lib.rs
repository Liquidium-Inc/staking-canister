use bitcoin::{consensus::deserialize, psbt::Psbt, Address, Network, Transaction};
use ordinals::{Artifact, Runestone};
use serde::{Deserialize, Serialize};
use std::error::Error;

// Legacy compatibility types for existing validation code
pub mod models {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RuneData {
        pub edicts: Vec<Edict>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Edict {
        pub id: String,
        pub amount: u128,
        pub output: u32,
        pub address: Option<String>,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunestoneOutput {
    pub edicts: Vec<EdictOutput>,
    pub etching: Option<EtchingOutput>,
    pub mint: Option<RuneIdOutput>,
    pub pointer: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdictOutput {
    pub id: RuneIdOutput,
    pub amount: u128,
    pub output: u32,
    pub address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuneIdOutput {
    pub block: u64,
    pub tx: u32,
}

impl std::fmt::Display for RuneIdOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.block, self.tx)
    }
}

impl PartialEq<String> for RuneIdOutput {
    fn eq(&self, other: &String) -> bool {
        self.to_string() == *other
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtchingOutput {
    pub divisibility: Option<u8>,
    pub premine: Option<u128>,
    pub rune: Option<String>,
    pub spacers: Option<u32>,
    pub symbol: Option<char>,
    pub terms: Option<TermsOutput>,
    pub turbo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermsOutput {
    pub amount: Option<u128>,
    pub cap: Option<u128>,
    pub height: Option<(Option<u64>, Option<u64>)>,
    pub offset: Option<(Option<u64>, Option<u64>)>,
}

/// Parse rune data from a base64 encoded PSBT
pub fn parse_psbt_runes(base64_data: &str) -> Result<RunestoneOutput, Box<dyn Error>> {
    let psbt_bytes =
        base64::decode(base64_data.trim()).map_err(|e| format!("Base64 decode error: {}", e))?;
    let psbt = Psbt::deserialize(&psbt_bytes)?;

    decode_runestone_from_psbt(&psbt)
}

/// Parse rune data from a base64 encoded PSBT with debug logging
pub fn parse_psbt_runes_with_debug(
    base64_data: &str,
) -> Result<(RunestoneOutput, String), Box<dyn Error>> {
    let mut debug_log = String::new();

    let psbt_bytes = match base64::decode(base64_data.trim()) {
        Ok(bytes) => {
            debug_log.push_str(&format!(
                "Base64 decoded successfully ({} bytes)\n",
                bytes.len()
            ));
            bytes
        }
        Err(e) => {
            debug_log.push_str(&format!("Base64 decode error: {}\n", e));
            return Err(debug_log.into());
        }
    };

    let psbt = match Psbt::deserialize(&psbt_bytes) {
        Ok(psbt) => {
            debug_log.push_str("PSBT deserialized successfully\n");
            psbt
        }
        Err(e) => {
            debug_log.push_str(&format!("PSBT deserialize error: {}\n", e));
            return Err(debug_log.into());
        }
    };

    match decode_runestone_from_psbt(&psbt) {
        Ok(runestone) => {
            debug_log.push_str(&format!(
                "Runestone decoded successfully: {:?}\n",
                runestone
            ));
            Ok((runestone, debug_log))
        }
        Err(e) => {
            debug_log.push_str(&format!("Runestone decode error: {}\n", e));
            Err(debug_log.into())
        }
    }
}

pub fn decode_runestone_from_tx(txhex: String) -> Result<RunestoneOutput, Box<dyn Error>> {
    let hex = hex::decode(txhex).expect("could not decode tx");
    let tx: Transaction = deserialize(&hex).expect("could not decode tx");
    match Runestone::decipher(&tx) {
        Some(artifact) => {
            let runestone = match artifact {
                Artifact::Runestone(runestone) => runestone,
                _ => return Err("Invalid artifact type".into()),
            };

            let mut edicts: Vec<EdictOutput> = runestone
                .edicts
                .iter()
                .map(|edict| EdictOutput {
                    id: RuneIdOutput {
                        block: edict.id.block,
                        tx: edict.id.tx,
                    },
                    amount: edict.amount,
                    output: edict.output,
                    address: None, // Will be filled below
                })
                .collect();

            // Add address information to edicts
            for edict in &mut edicts {
                if (edict.output as usize) >= tx.output.len() {
                    return Err("bug: invalid tx".to_string().into());
                }

                if (edict.output as usize) < tx.output.len() {
                    let output = &tx.output[edict.output as usize];
                    if let Ok(address) =
                        Address::from_script(&output.script_pubkey, Network::Bitcoin)
                    {
                        edict.address = Some(address.to_string());
                    } else if let Ok(address) =
                        Address::from_script(&output.script_pubkey, Network::Testnet)
                    {
                        edict.address = Some(address.to_string());
                    }
                }
            }

            let etching = runestone.etching.map(|etching| EtchingOutput {
                divisibility: etching.divisibility,
                premine: etching.premine,
                rune: etching.rune.map(|r| r.to_string()),
                spacers: etching.spacers,
                symbol: etching.symbol,
                terms: etching.terms.map(|terms| TermsOutput {
                    amount: terms.amount,
                    cap: terms.cap,
                    height: Some(terms.height),
                    offset: Some(terms.offset),
                }),
                turbo: etching.turbo,
            });

            let mint = runestone.mint.map(|mint| RuneIdOutput {
                block: mint.block,
                tx: mint.tx,
            });

            Ok(RunestoneOutput {
                edicts,
                etching,
                mint,
                pointer: runestone.pointer,
            })
        }
        None => Err("No runestone found in transaction".into()),
    }
}

fn decode_runestone_from_psbt(psbt: &Psbt) -> Result<RunestoneOutput, Box<dyn Error>> {
    let tx = &psbt.unsigned_tx;

    match Runestone::decipher(tx) {
        Some(artifact) => {
            let runestone = match artifact {
                Artifact::Runestone(runestone) => runestone,
                _ => return Err("Invalid artifact type".into()),
            };

            let mut edicts: Vec<EdictOutput> = runestone
                .edicts
                .iter()
                .map(|edict| EdictOutput {
                    id: RuneIdOutput {
                        block: edict.id.block,
                        tx: edict.id.tx,
                    },
                    amount: edict.amount,
                    output: edict.output,
                    address: None, // Will be filled below
                })
                .collect();

            // Add address information to edicts
            for edict in &mut edicts {
                if (edict.output as usize) >= tx.output.len() {
                    return Err("bug: invalid tx".to_string().into());
                }
                
                if (edict.output as usize) < tx.output.len() {
                    let output = &tx.output[edict.output as usize];
                    if let Ok(address) =
                        Address::from_script(&output.script_pubkey, Network::Bitcoin)
                    {
                        edict.address = Some(address.to_string());
                    } else if let Ok(address) =
                        Address::from_script(&output.script_pubkey, Network::Testnet)
                    {
                        edict.address = Some(address.to_string());
                    }
                }
            }

            let etching = runestone.etching.map(|etching| EtchingOutput {
                divisibility: etching.divisibility,
                premine: etching.premine,
                rune: etching.rune.map(|r| r.to_string()),
                spacers: etching.spacers,
                symbol: etching.symbol,
                terms: etching.terms.map(|terms| TermsOutput {
                    amount: terms.amount,
                    cap: terms.cap,
                    height: Some(terms.height),
                    offset: Some(terms.offset),
                }),
                turbo: etching.turbo,
            });

            let mint = runestone.mint.map(|mint| RuneIdOutput {
                block: mint.block,
                tx: mint.tx,
            });

            Ok(RunestoneOutput {
                edicts,
                etching,
                mint,
                pointer: runestone.pointer,
            })
        }
        None => Err("No runestone found in transaction".into()),
    }
}

// Convert from RunestoneOutput format to legacy RuneData format
pub fn convert_runestone_to_rune_data(runestone: RunestoneOutput) -> models::RuneData {
    let edicts = runestone
        .edicts
        .into_iter()
        .map(|edict| models::Edict {
            id: format!("{}:{}", edict.id.block, edict.id.tx),
            amount: edict.amount,
            output: edict.output,
            address: edict.address,
        })
        .collect();

    models::RuneData { edicts }
}

/// Parse PSBT runes and return legacy format
pub fn parse_psbt_runes_legacy(base64_data: &str) -> Result<models::RuneData, Box<dyn Error>> {
    let runestone = parse_psbt_runes(base64_data)?;
    let rune_data = convert_runestone_to_rune_data(runestone);
    Ok(rune_data)
}

/// Parse PSBT runes with debug logging and return legacy format
pub fn parse_psbt_runes_with_debug_legacy(
    base64_data: &str,
) -> Result<(models::RuneData, String), Box<dyn Error>> {
    let (runestone, debug_log) = parse_psbt_runes_with_debug(base64_data)?;
    let rune_data = convert_runestone_to_rune_data(runestone);
    Ok((rune_data, debug_log))
}
