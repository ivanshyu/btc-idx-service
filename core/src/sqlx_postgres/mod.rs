pub mod bitcoin;

use std::str::FromStr;

use serde::{de::DeserializeOwned, Serialize};
use sqlx::{
    migrate::Migrator,
    postgres::{PgPoolOptions, PgQueryResult, PgRow},
    types::Json,
    Error as SqlxError, PgPool, Postgres, Row,
};

pub static EMBEDDED_MIGRATE: Migrator = sqlx::migrate!();

pub async fn connect_and_migrate(database_url: &str, max_connections: u32) -> sqlx::Result<PgPool> {
    create_database(database_url).await?;
    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await?;

    EMBEDDED_MIGRATE.run(&pool).await?;
    Ok(pool)
}

pub async fn create_database(uri: &str) -> sqlx::Result<()> {
    use sqlx::any::Any;
    use sqlx::migrate::MigrateDatabase;

    log::info!("Creating database: {}", uri);
    sqlx::any::install_default_drivers();
    if !Any::database_exists(uri).await? {
        Any::create_database(uri).await
    } else {
        Ok(())
    }
}

pub fn ensure_affected(count: u64) -> impl FnOnce(PgQueryResult) -> sqlx::Result<()> {
    move |pg_done| {
        if pg_done.rows_affected() == count {
            Ok(())
        } else {
            Err(SqlxError::RowNotFound)
        }
    }
}
