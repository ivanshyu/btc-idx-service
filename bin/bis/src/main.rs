mod api;
mod cli;
mod config;
pub mod mono;
pub mod tools;

use atb::logging::init_logger;
use atb_cli::AtbCli;
use cli::{Cli, Subcommand};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    Cli::set_globals(&cli.base);

    init_logger(
        "info,sqlx=info",
        cli.base.debug || atb_cli::process_info().env().dev(),
    );
    log::info!("{:?}", atb_cli::process_info());

    use Subcommand::*;
    match cli.subcommand {
        Mono(opts) => mono::run(cli.shared_params, opts),
        Indexer(opts) => tools::run(opts),
    }
}
