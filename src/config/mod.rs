use bitcoin::Network;
use candid::CandidType;
use ic_cdk::bitcoin_canister::Network as BitcoinNetwork;
use serde::{Deserialize, Serialize};

/// Application configuration
#[derive(CandidType, Deserialize, Serialize)]
pub struct AppConfig {
    /// Bitcoin network settings
    pub bitcoin: BitcoinConfig,
}

/// Bitcoin network settings
#[derive(CandidType, Deserialize, Serialize)]
pub struct BitcoinConfig {
    /// Bitcoin network type (only Mainnet is supported)
    pub network: BitcoinNetwork,
    /// LIQ rune ID
    pub liq_rune_id: String,
    /// sLIQ rune ID
    pub sliq_rune_id: String,
    /// Withdrawal lockup period in seconds (default: 7 days)
    pub withdrawal_lockup_period: u64,
}

/// Returns default application configuration
pub fn default_config() -> AppConfig {
    #[cfg(feature = "prod")]
    {
        AppConfig {
            bitcoin: BitcoinConfig {
                network: BitcoinNetwork::Mainnet,
                liq_rune_id: String::from("840010:907"),
                sliq_rune_id: String::from("889844:1179"),
                withdrawal_lockup_period: 604800, // 604800 = 7 days in seconds
            },
        }
    }

    #[cfg(not(feature = "prod"))]
    {
        AppConfig {
            bitcoin: BitcoinConfig {
                network: BitcoinNetwork::Mainnet, // or Testnet
                liq_rune_id: String::from("846186:222"),
                sliq_rune_id: String::from("905811:182"),
                withdrawal_lockup_period: 60,
            },
        }
    }
}

/// Converts between network types
pub fn to_bitcoin_network(network: BitcoinNetwork) -> Network {
    match network {
        BitcoinNetwork::Mainnet => Network::Bitcoin,
        BitcoinNetwork::Testnet => Network::Testnet,
        BitcoinNetwork::Regtest => Network::Regtest,
    }
}
