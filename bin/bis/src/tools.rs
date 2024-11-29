use std::path::PathBuf;

use atb_cli::clap::{self, Parser, ValueHint};
use reqwest::Client;

#[derive(Parser, Debug, Clone)]
pub struct Opts {
    #[clap(subcommand)]
    subcommand: Subcommand,

    /// Host string in "${HOST}:${PORT}" format.
    #[clap(long, default_value = "127.0.0.1:3030", env = "BIS_HOST")]
    host: String,

    #[clap(
        short,
        long,
        env = "BIS_CONFIG_FILE",
        parse(from_os_str),
        value_hint = ValueHint::FilePath
    )]
    config_file: Option<PathBuf>,
}

#[derive(Parser, Debug, Clone)]
pub enum Subcommand {
    ScanBlock {
        #[clap(short, long, env = "FROM")]
        from: usize,
        #[clap(short, long, env = "TO")]
        to: usize,
    },
}

pub fn run(opts: Opts) -> anyhow::Result<()> {
    match opts.subcommand {
        Subcommand::ScanBlock { from, to } => scan_block(opts, from, to),
    }
}

fn scan_block(opts: Opts, from: usize, to: usize) -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime needed to continue. qed");

    rt.block_on(async {
        let client = Client::new();
        let url = format!(
            "http://{}/api/v1/protected/harvester/scan/{}/{}",
            opts.host, from, to
        );

        let response = client.post(&url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "Failed to scan blocks: {} - {}",
                response.status(),
                response.text().await?
            );
        }
        Ok(())
    })
}
