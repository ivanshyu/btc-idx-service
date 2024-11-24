use atb_types::DateTime;
use bitcoin::BlockHash;
use sqlx::{
    migrate::Migrator,
    postgres::{PgPoolOptions, PgQueryResult, PgRow},
    types::{BigDecimal, Json},
    Error as SqlxError, Executor, PgPool, Postgres, Row,
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

pub async fn has_block<'e, T>(conn: T, hash: &BlockHash) -> Result<bool, sqlx::Error>
where
    T: sqlx::Executor<'e, Database = sqlx::postgres::Postgres>,
{
    sqlx::query(
        r#"
            SELECT EXISTS(SELECT 1 FROM blocks WHERE hash = $1)
        "#,
    )
    .bind(format!("{:?}", hash))
    .map(|row: PgRow| -> bool { row.get(0) })
    .fetch_one(conn)
    .await
}

pub async fn upsert_block<'e, T>(
    conn: T,
    hash: &BlockHash,
    prev_hash: Option<&BlockHash>,
    number: usize,
    timestamp: DateTime,
    nonce: &BigDecimal,
    version: i32,
    difficulty: &BigDecimal,
) -> Result<(), sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    log::info!("upsert block, {}", number);
    sqlx::query(
        r#"
            INSERT INTO btc_blocks (hash, number, previous_hash, timestamp, nonce, version, difficulty) 
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT DO NOTHING
        "#,
    )
    .bind(format!("{:x}", hash.as_raw_hash()))
    .bind(BigDecimal::from(number as u32))
    .bind(prev_hash.map(|h| format!("{:x}", h.as_raw_hash())).as_deref())
    .bind(timestamp)
    .bind(nonce)
    .bind(version)
    .bind(difficulty)
    .execute(conn)
    .await
    .map(|_| ())
}

pub async fn upsert_transaction<'e, T>(
    conn: T,
    txid: &bitcoin::Txid,
    block_hash: &BlockHash,
    transaction_index: i32,
    lock_time: &BigDecimal,
    is_coinbase: bool,
    version: i32,
) -> Result<(), sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
            INSERT INTO btc_transactions (txid, block_hash, transaction_index, lock_time, is_coinbase, version)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT ON CONSTRAINT btc_transactions_pkey
            DO UPDATE SET block_hash = $1
        "#,
    )
    .bind(format!("{:?}", txid))
    .bind(format!("{:x}", block_hash.as_raw_hash()))
    .bind(transaction_index)
    .bind(lock_time)
    .bind(is_coinbase)
    .bind(version)
    .execute(conn)
    .await
    .map(|_| ())
}
