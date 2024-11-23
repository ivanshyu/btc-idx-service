use crate::{
    api::{build_http_service, ServiceConfig},
    cli::SharedParams,
    config::Config,
};

use std::{path::PathBuf, sync::Arc};

use actix_web::rt::System;
use atb_cli::{
    clap::{self, Parser, ValueHint},
    Environment,
};
use atb_tokio_ext::TaskService;
use bis_core::{
    bitcoin::{
        client::{Client, Processor},
        harvester::{self, Harvester},
    },
    sqlx_postgres::connect_and_migrate,
};
use bitcoincore_rpc::{json, Auth};
use bitcoincore_rpc::{Client as InnerClient, RpcApi};
use sqlx::{migrate::Migrator, postgres::PgPoolOptions, PgPool};

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
}

pub fn run(shared_params: SharedParams, opts: Opts) -> anyhow::Result<()> {
    let config_file = opts
        .config_file
        .unwrap_or_else(|| "./deployment/config_dev.toml".into());
    let config = Config::from_file(&config_file)
        .map_err(|e| anyhow::anyhow!("failed to load configuration file {config_file:?}: {e}"))?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime needed to start. qed");

    let (mut server, mut all_services_complete) = TaskService::new();

    let service_config = rt.block_on(build_service_config(
        &mut server,
        config,
        &shared_params.database_url,
        atb_cli::process_info().env(),
        atb_cli::debug(),
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
    env: &Environment,
    debug: bool,
) -> anyhow::Result<ServiceConfig> {
    log::info!("Configuring bitcoin");
    let pg_pool: PgPool = connect_and_migrate(database_url, 5).await?.into();

    let pg_pool_cloned = pg_pool.clone();

    let client = Client::new(
        "bitcoin client".to_owned(),
        "http://localhost:18443",
        Some("user"),
        Some("password"),
    )?;

    let client = Arc::new(client);

    let processor = Processor::new(pg_pool_cloned, client.clone()).await?;

    let harvester = Harvester::new(
        client.clone(),
        processor,
        None,
        None,
        1000,
        "bitcoin harvester".to_owned(),
    );

    task_service.add_task(harvester.to_boxed_task_fn());

    Ok(ServiceConfig { pg_pool })
}
