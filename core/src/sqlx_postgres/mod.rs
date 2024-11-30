pub mod bitcoin;

use serde::{de::DeserializeOwned, Serialize};
use sqlx::{
    migrate::Migrator,
    postgres::{PgPoolOptions, PgQueryResult, PgRow},
    types::Json,
    Error as SqlxError, Executor, PgPool, Postgres, Row,
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

pub async fn upsert_config<'e, T, U>(conn: T, key: &str, data: U) -> Result<(), sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
    U: Serialize + Send,
{
    sqlx::query(
        r#"
        INSERT INTO config (key, data)
        VALUES ($1, $2)
        ON CONFLICT (key)
        DO UPDATE SET data = $2
        WHERE config.key = $1
        "#,
    )
    .bind(key)
    .bind(Json(data))
    .execute(conn)
    .await
    .and_then(ensure_affected(1))
}

pub async fn get_config<'e, T, U>(conn: T, key: &str) -> Result<Option<U>, sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
    U: DeserializeOwned + Unpin + Send,
{
    sqlx::query(
        r#"
        SELECT data FROM config
        WHERE key = $1
        "#,
    )
    .bind(key)
    .try_map(|row: PgRow| Ok(row.try_get::<Json<U>, _>(0)?.0))
    .fetch_optional(conn)
    .await
}
