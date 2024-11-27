use super::ensure_affected;
use crate::bitcoin::types::{BtcBalance, BtcP2trEvent, BtcUtxoInfo, BTC_NETWORK};

use std::str::FromStr;

use atb_types::DateTime;
use bitcoin::{Address, BlockHash, OutPoint, Txid};
use num_traits::{FromPrimitive, ToPrimitive};
use sqlx::{postgres::PgRow, types::BigDecimal, Error as SqlxError, Executor, Postgres, Row};

impl TryFrom<PgRow> for BtcUtxoInfo {
    type Error = sqlx::Error;
    fn try_from(row: PgRow) -> Result<Self, Self::Error> {
        let txid: &str = row.get(2);
        let txid = Txid::from_str(txid).map_err(|e| SqlxError::Decode(e.into()))?;

        let vout = row.get::<i64, _>(3) as u32;

        let amount: BigDecimal = row.get(4);

        let spent_block = row
            .get::<Option<BigDecimal>, _>(6)
            .map(|bd| {
                bd.to_u64()
                    .ok_or_else(|| SqlxError::Decode("convert bigdecimal to u64 failed".into()))
            })
            .transpose()?;

        Ok(BtcUtxoInfo {
            owner: row.get(1),
            txid,
            vout,
            amount,
            spent_block,
        })
    }
}

impl TryFrom<PgRow> for BtcBalance {
    type Error = sqlx::Error;

    fn try_from(row: PgRow) -> Result<Self, Self::Error> {
        let address: &str = row.get(0);
        let address = Address::from_str(address)
            .map_err(|e| SqlxError::Decode(e.into()))?
            .require_network(*BTC_NETWORK.get().unwrap())
            .map_err(|e| SqlxError::Decode(e.into()))?;

        let amount: BigDecimal = row.get(1);

        Ok(BtcBalance { address, amount })
    }
}

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
    .bind(BigDecimal::from(number as u64))
    .bind(prev_hash.map(|h| format!("{:x}", h.as_raw_hash())).as_deref())
    .bind(timestamp)
    .bind(nonce)
    .bind(version)
    .bind(difficulty)
    .execute(conn)
    .await
    .and_then(ensure_affected(1))
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
    .and_then(ensure_affected(1))
}

pub async fn create_utxo<'e, T>(
    conn: T,
    owner: &str,
    txid: Txid,
    vout: usize,
    amount: &BigDecimal,
    block_num: usize,
) -> Result<(), sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    // Create id
    let id = format!("{}:{}", txid, vout);

    sqlx::query(
        r#"
        INSERT INTO btc_utxos
        (id, address, txid, vout, amount, block_number)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(id)
    .bind(owner)
    .bind(txid.to_string())
    .bind(vout as i64)
    .bind(amount)
    .bind(BigDecimal::from(block_num as u64))
    .execute(conn)
    .await
    .and_then(ensure_affected(1))
}

async fn get_utxos_at_block<'e, T>(
    conn: T,
    block_num: usize,
) -> Result<Vec<BtcUtxoInfo>, sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    let block_num = BigDecimal::from_u64(block_num as u64);

    sqlx::query(
        r#"
        SELECT id, address, txid, vout, amount, block_number, spent_block 
        FROM btc_utxos
        WHERE block_number = $1
        "#,
    )
    .bind(block_num)
    .try_map(BtcUtxoInfo::try_from)
    .fetch_all(conn)
    .await
    .map_err(Into::into)
}

async fn get_utxos_spent_at_block<'e, T>(
    conn: T,
    block_num: usize,
) -> Result<Vec<BtcUtxoInfo>, sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    let block_num = BigDecimal::from_u64(block_num as u64);

    sqlx::query(
        r#"
        SELECT address, txid, vout, amount, spent_block 
        FROM btc_utxos
        WHERE spent_block = $1
        "#,
    )
    .bind(block_num)
    .try_map(BtcUtxoInfo::try_from)
    .fetch_all(conn)
    .await
    .map_err(Into::into)
}

async fn get_unspent_utxos_by_owner<'e, T>(
    conn: T,
    owner: &str,
) -> Result<Vec<BtcUtxoInfo>, sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        SELECT address, txid, vout, amount, spent_block 
        FROM btc_utxos
        WHERE address = $1 AND spent_block IS NULL
        "#,
    )
    .bind(owner)
    .try_map(BtcUtxoInfo::try_from)
    .fetch_all(conn)
    .await
    .map_err(Into::into)
}

pub async fn get_relevant_utxos<'e, T>(
    conn: T,
    utxos: &[&OutPoint],
) -> Result<Vec<BtcUtxoInfo>, sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    let ids: Vec<String> = utxos
        .iter()
        .map(|x| format!("{}:{}", x.txid, x.vout))
        .collect();

    sqlx::query(
        r#"
        SELECT id, address, txid, vout, amount, block_number, spent_block 
        FROM btc_utxos
        WHERE spent_block IS NULL AND id = ANY($1) 
        "#,
    )
    .bind(ids)
    .try_map(BtcUtxoInfo::try_from)
    .fetch_all(conn)
    .await
}

pub async fn remove_utxo<'e, T>(conn: T, txid: Txid, vout: u32) -> Result<(), sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
            DELETE FROM btc_utxos
            WHERE txid = $1 AND vout = $2
        "#,
    )
    .bind(txid.to_string())
    .bind(vout as i64)
    .execute(conn)
    .await
    .and_then(ensure_affected(1))
}

pub async fn remove_utxos_since_block<'e, T>(conn: T, block_num: usize) -> Result<u64, sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    let block_num = BigDecimal::from_u64(block_num as u64);

    sqlx::query(
        r#"
            DELETE FROM btc_utxos
            WHERE block_number >= $1
        "#,
    )
    .bind(block_num)
    .execute(conn)
    .await
    .map(|pg_done| pg_done.rows_affected())
}

pub async fn spend_utxo<'e, T>(
    conn: T,
    txid: Txid,
    vout: u32,
    block_num: usize,
) -> Result<(), sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    let block_num = BigDecimal::from_u64(block_num as u64);

    sqlx::query(
        r#"
            UPDATE btc_utxos
            SET spent_block = $1
            WHERE txid = $2 AND vout = $3
        "#,
    )
    .bind(block_num)
    .bind(txid.to_string())
    .bind(vout as i64)
    .execute(conn)
    .await
    .and_then(ensure_affected(1))
}

pub async fn unspend_utxo<'e, T>(conn: T, txid: Txid, vout: u32) -> Result<(), sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
            UPDATE btc_utxos
            SET spent_block = NULL
            WHERE txid = $1 AND vout = $2
        "#,
    )
    .bind(txid.to_string())
    .bind(vout as i64)
    .execute(conn)
    .await
    .and_then(ensure_affected(1))
}

pub async fn get_last_processed_event_id<'e, T>(conn: T) -> Result<u64, sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
            SELECT sequence_id
            FROM btc_p2tr_events
            ORDER BY sequence_id DESC LIMIT 1
            "#,
    )
    .try_map(|row: PgRow| {
        let index = row.try_get::<i64, _>(0).unwrap_or(0);
        index
            .try_into()
            .map_err(|_| sqlx::Error::Decode("convert i64 to u64 failed".into()))
    })
    .fetch_one(conn)
    .await
}

pub async fn create_p2tr_event<'e, T>(conn: T, event: BtcP2trEvent) -> Result<u64, sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    let block_num = BigDecimal::from_u64(event.block_number as u64);

    sqlx::query(
        r#"
        INSERT INTO btc_p2tr_events 
        (block_number, tx_hash, address, amount, action) 
        VALUES ($1, $2, $3, $4, $5)
        RETURNING sequence_id
        "#,
    )
    .bind(block_num)
    .bind(event.txid.to_string())
    .bind(event.address.to_string())
    .bind(event.amount)
    .bind(event.action as i16)
    .try_map(|row: PgRow| {
        let index = row.try_get::<i64, _>(0).unwrap_or(0);
        index
            .try_into()
            .map_err(|_| sqlx::Error::Decode("convert i64 to u64 failed".into()))
    })
    .fetch_one(conn)
    .await
}

pub async fn get_btc_balance<'e, T>(
    conn: T,
    address: &str,
) -> Result<Option<BtcBalance>, sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
        SELECT address, balance
        FROM btc_balances
        WHERE address = $1
        "#,
    )
    .bind(address)
    .try_map(BtcBalance::try_from)
    .fetch_optional(conn)
    .await
    .map_err(Into::into)
}

pub async fn increment_btc_balance<'e, T>(
    conn: T,
    address: &str,
    value: &BigDecimal,
    timestamp: DateTime,
) -> Result<(), sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
            INSERT INTO btc_balances AS b
                (address, balance, last_updated) 
                VALUES ($1, $2, $3)
            ON CONFLICT ON CONSTRAINT btc_balances_pkey 
                DO UPDATE SET balance = b.balance + $2, last_updated = $3;
        "#,
    )
    .bind(address)
    .bind(value)
    .bind(timestamp)
    .execute(conn)
    .await
    .and_then(ensure_affected(1))
}

pub async fn increment_static_balances<'e, T>(
    conn: T,
    address: &str,
    value: &BigDecimal,
    current_hour: DateTime,
    timestamp: DateTime,
) -> Result<(), sqlx::Error>
where
    T: Executor<'e, Database = Postgres>,
{
    sqlx::query(
        r#"
            INSERT INTO statistic_btc_balances AS b
                (address, balance, datetime_hour, last_updated) 
                VALUES ($1, $2, $3, $4)
            ON CONFLICT ON CONSTRAINT statistic_btc_balances_pkey 
                DO UPDATE SET balance = b.balance + $2, last_updated = $3;
        "#,
    )
    .bind(address)
    .bind(value)
    .bind(current_hour)
    .bind(timestamp)
    .execute(conn)
    .await
    .and_then(ensure_affected(1))
}
