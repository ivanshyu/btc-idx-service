use atb_cli::{
    clap::{self, Parser},
    AtbCli, BaseCli,
};

#[derive(Parser, Debug)]
#[clap(name = "BIS", about = "BIS")]
pub struct Cli {
    #[clap(flatten)]
    pub base: BaseCli,

    #[clap(flatten)]
    pub shared_params: SharedParams,

    /// Subcommands
    #[clap(subcommand)]
    pub subcommand: Subcommand,
}

impl Cli {
    #[allow(unused)]
    pub fn create_runtime(
        worker_threads: Option<usize>,
    ) -> anyhow::Result<tokio::runtime::Runtime> {
        let mut builder = tokio::runtime::Builder::new_multi_thread();
        if let Some(n) = worker_threads {
            builder.worker_threads(n);
        }
        builder.enable_all().build().map_err(Into::into)
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Parser)]
pub enum Subcommand {
    /// Monolithic API
    Mono(crate::mono::Opts),
}

#[derive(Debug, Parser)]
pub struct SharedParams {
    /// Tokio Worker Threads
    #[clap(env = "BIS_WORKER_THREADS")]
    pub worker_threads: Option<usize>,

    /// Database connection
    #[clap(
        name = "db",
        default_value = "postgres://postgres:123456@localhost:5432/postgres?sslmode=disable",
        env = "BIS_POSTGRES"
    )]
    pub database_url: String,

    /// Database Pool Size
    #[clap(default_value = "5", env = "BIS_POSTGRES_POOL_SIZE")]
    pub database_pool_size: u32,
}

impl AtbCli for Cli {
    /// Application name.
    fn name() -> String {
        env!("CARGO_PKG_NAME").to_owned()
    }

    /// Application version.
    fn version() -> String {
        env!("CARGO_PKG_VERSION").to_owned()
    }

    /// Application authors.
    fn authors() -> Vec<String> {
        let authors = env!("CARGO_PKG_AUTHORS");
        authors
            .split(':')
            .map(str::to_string)
            .collect::<Vec<String>>()
    }

    /// Application description.
    fn description() -> String {
        env!("CARGO_PKG_DESCRIPTION").to_owned()
    }

    /// Application repository.
    fn repository() -> String {
        env!("CARGO_PKG_REPOSITORY").to_owned()
    }

    /// Returns implementation details.
    fn impl_version() -> String {
        "".to_owned()
    }

    /// Returns git commit hash(short).  By default, expects build time helpers using atb-rs/utils/build
    fn commit() -> String {
        "".to_owned()
    }

    /// Returns git branch.  By default, expects build time helpers using atb-rs/utils/build
    fn branch() -> String {
        "".to_owned()
    }

    /// Returns OS platform.  By default, expects build time helpers using atb-rs/utils/build
    fn platform() -> String {
        "".to_owned()
    }

    /// Returns rustc version.  By default, expects build time helpers using atb-rs/utils/build
    fn rustc_info() -> String {
        "".to_owned()
    }
}
