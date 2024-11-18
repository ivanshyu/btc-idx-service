use crate::{
    api::{build_http_service, ServiceConfig},
    cli::SharedParams,
    config::Config,
};

use std::path::PathBuf;

use actix_web::rt::System;
use atb_cli::{
    clap::{self, Parser, ValueHint},
    Environment,
};
use atb_tokio_ext::TaskService;
use bis_core::sqlx_postgres::connect_and_migrate;
use sqlx::{migrate::Migrator, postgres::PgPoolOptions, PgPool};

#[derive(Parser, Debug, Clone)]
pub struct Opts {
    /// Host string in "${HOST}:${PORT}" format.
    #[clap(long, default_value = "127.0.0.1:3030", env = "BIS_HOST")]
    host: String,

    /// Fully qualified domain name
    #[clap(long, default_value = "localhost", env = "BIS_FQDN")]
    fqdn: String,

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
    assert!(
        &shared_params.database_url.contains("{}"),
        "database url without string interpolation parameter is not allowed."
    );

    let config_file = opts
        .config_file
        .unwrap_or_else(|| "./deployment/bis_dev.toml".into());
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
    log::info!("Configuring ethereum");
    let pg_pool = connect_and_migrate(database_url, 5).await?.into();

    // let (harvester, provider) = new_btc_harvester(store.clone(), env, "bitcoin").await?;
    // let handle = harvester.handle();
    // task_service.add_task(harvester.to_boxed_task_fn());

    Ok(ServiceConfig { pg_pool })
}
