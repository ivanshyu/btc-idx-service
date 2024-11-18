use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
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