use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// The magic number for the Bitcoin network
    /// 0xD9B4BEF9(3652501241) => Some(Network::Bitcoin),
    /// 0x0709110B(118034699) => Some(Network::Testnet),
    /// 0x40CF030A(1087308554) => Some(Network::Signet),
    /// 0xDAB5BFFA(3669344250) => Some(Network::Regtest),
    pub magic: u32,
    pub provider_url: String,
    pub poll_frequency_ms: u64,
    pub start_block: Option<u64>,
}

impl Config {
    /// load configuration from filepath
    pub fn from_file(file_path: &PathBuf) -> anyhow::Result<Self> {
        let config_toml = fs::read_to_string(file_path)?;
        toml::from_str(&config_toml).map_err(Into::into)
    }
}
