use sqlx::{
    migrate::Migrator,
    postgres::{PgPoolOptions, PgQueryResult, PgRow},
    types::Json,
    Error as SqlxError, PgPool, Postgres, Row,
};

pub async fn get_latest_sequence_id<'e, T>(conn: T) -> Result<i64, sqlx::Error>
where
    T: sqlx::Executor<'e, Database = sqlx::postgres::Postgres>,
{
    sqlx::query(
        r#"
        SELECT sequence_id
        FROM events 
        ORDER BY sequence_id DESC LIMIT 1
    "#,
    )
    .try_map(|row: PgRow| row.try_get(0))
    .fetch_optional(conn)
    .await
    .map(|v| v.unwrap_or_default())
}
