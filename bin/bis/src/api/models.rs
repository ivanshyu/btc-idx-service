use bis_core::bitcoin::types::{BtcBalance, BtcBlock, BtcTransaction, BtcUtxoInfo};

use atb_cli::DateTime;
use bitcoin::{BlockHash, Txid};
use serde::{Deserialize, Serialize};
use sqlx::types::BigDecimal;

#[derive(Serialize)]
pub struct LatestBlockHeight {
    pub latest_block_height: Option<usize>,
}

#[derive(Serialize)]
pub struct BlockHeader {
    pub block_header_data: BlockHeaderData,
}

#[derive(Serialize)]
pub struct BlockHeaderData {
    pub hash: BlockHash,
    pub number: usize,
    pub previous_hash: BlockHash,
    pub timestamp: DateTime,
    pub nonce: i64,
    pub version: i32,
    pub difficulty: BigDecimal,
}

impl From<BtcBlock> for BlockHeaderData {
    fn from(block: BtcBlock) -> Self {
        BlockHeaderData {
            hash: block.hash,
            number: block.number,
            previous_hash: block.previous_hash,
            timestamp: block.timestamp,
            nonce: block.nonce,
            version: block.version,
            difficulty: block.difficulty,
        }
    }
}
#[derive(Serialize)]
pub struct Transaction {
    pub transaction_data: TransactionData,
}

#[derive(Serialize)]
pub struct TransactionData {
    pub txid: Txid,
    pub block_hash: BlockHash,
    pub transaction_index: usize,
    pub lock_time: u64,
    pub version: i32,
}

impl From<BtcTransaction> for TransactionData {
    fn from(transaction: BtcTransaction) -> Self {
        TransactionData {
            txid: transaction.txid,
            block_hash: transaction.block_hash,
            transaction_index: transaction.transaction_index,
            lock_time: transaction.lock_time,
            version: transaction.version,
        }
    }
}
#[derive(Serialize)]
pub struct CurrentBalance {
    pub current_balance: BigDecimal,
}

impl From<BtcBalance> for CurrentBalance {
    fn from(balance: BtcBalance) -> Self {
        CurrentBalance {
            current_balance: balance.amount,
        }
    }
}

#[derive(Serialize)]
pub struct Utxo {
    pub transaction_id: Txid,
    pub transaction_index: usize,
    pub satoshi: BigDecimal,
    pub block_height: usize,
}

impl From<BtcUtxoInfo> for Utxo {
    fn from(utxo: BtcUtxoInfo) -> Self {
        Utxo {
            transaction_id: utxo.txid,
            transaction_index: utxo.vout as usize,
            satoshi: utxo.amount,
            block_height: utxo.block_number as usize,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AggregatedBalanceParams {
    pub time_span: TimeSpan,
    pub granularity: Granularity,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeSpan {
    // Recent Month
    M,
    // Recent Week
    W,
    // Recent Day
    D,
}

impl std::fmt::Display for TimeSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeSpan::M => write!(f, "m"),
            TimeSpan::W => write!(f, "w"),
            TimeSpan::D => write!(f, "d"),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Granularity {
    // Weekly
    W,
    // Daily
    D,
    // Hourly
    H,
}

impl std::fmt::Display for Granularity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Granularity::W => write!(f, "w"),
            Granularity::D => write!(f, "d"),
            Granularity::H => write!(f, "h"),
        }
    }
}

#[derive(Serialize)]
pub struct AggregatedBalance {
    pub balance: BigDecimal,
}
