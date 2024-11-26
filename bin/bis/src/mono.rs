use crate::{
    api::{build_http_service, ServiceConfig},
    cli::SharedParams,
    config::Config,
};

use std::{path::PathBuf, str::FromStr, sync::Arc};

use actix_web::rt::System;
use atb_cli::clap::{self, Parser, ValueHint};
use atb_tokio_ext::TaskService;
use bis_core::{
    bitcoin::{
        client::{Client, Processor},
        harvester::Harvester,
    },
    sqlx_postgres::connect_and_migrate,
};
use bitcoin::{p2p::Magic, Network};
use once_cell::sync::OnceCell;
use sqlx::PgPool;

pub static BTC_NETWORK: OnceCell<Network> = OnceCell::new();

#[derive(Parser, Debug, Clone)]
pub struct Opts {
    /// Host string in "${HOST}:${PORT}" format.
    #[clap(long, default_value = "127.0.0.1:3030", env = "BIS_HOST")]
    host: String,

    /// The service name to use for Apm
    #[clap(
        short,
        long,
        env = "BIS_CONFIG_FILE",
        parse(from_os_str),
        value_hint = ValueHint::FilePath
    )]
    config_file: Option<PathBuf>,

    #[clap(long, env = "BTCUSER")]
    rpc_user: Option<String>,

    #[clap(long, env = "BTCUSERPASSWORD")]
    rpc_pwd: Option<String>,
}

pub fn run(shared_params: SharedParams, opts: Opts) -> anyhow::Result<()> {
    let config_file = opts
        .config_file
        .unwrap_or_else(|| "./deployment/config.toml".into());
    let config = Config::from_file(&config_file)
        .map_err(|e| anyhow::anyhow!("failed to load configuration file {config_file:?}: {e}"))?;

    let network = Network::from_core_arg(&config.magic).unwrap();
    BTC_NETWORK
        .set(network)
        .expect("BTC_NETWORK should not be set");

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime needed to start. qed");

    let (mut server, mut all_services_complete) = TaskService::new();

    let service_config = rt.block_on(build_service_config(
        &mut server,
        config,
        &shared_params.database_url,
        opts.rpc_user,
        opts.rpc_pwd,
        network,
    ))?;

    // Run services to completion
    rt.spawn(server.run(tokio::signal::ctrl_c()));
    System::new().block_on(build_http_service(&opts.host, service_config))?;

    rt.block_on(all_services_complete.recv());
    Ok(())
}

pub async fn build_service_config(
    task_service: &mut TaskService,
    config: crate::config::Config,
    database_url: &str,
    rpc_user: Option<String>,
    rpc_pwd: Option<String>,
    network: Network,
) -> anyhow::Result<ServiceConfig> {
    log::info!("Configuring bitcoin");
    let pg_pool: PgPool = connect_and_migrate(database_url, 5).await?.into();

    let pg_pool_cloned = pg_pool.clone();

    log::info!("Connected to bitcoin rpc: {}", &config.provider_url);

    let client = Client::new(
        "bitcoin client".to_owned(),
        config.provider_url,
        rpc_user,
        rpc_pwd,
    );

    let client = Arc::new(client);

    let processor = Processor::new(pg_pool_cloned, client.clone(), network).await?;

    let harvester = Harvester::new(
        client.clone(),
        processor,
        None,
        None,
        config.poll_frequency_ms,
        "bitcoin harvester".to_owned(),
    );

    task_service.add_task(harvester.to_boxed_task_fn());

    Ok(ServiceConfig { pg_pool })
}
