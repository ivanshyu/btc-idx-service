use bis_core::bitcoin::types::{BtcBalance, BtcBlock, BtcTransaction, BtcUtxoInfo};

use std::collections::HashMap;

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
pub enum TimeSpan {
    // Recent Month
    M,
    // Recent Week
    W,
    // Recent Day
    D,
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

pub type AggregatedInfo = HashMap<DateTime, Vec<AggregatedBalance>>;

#[derive(Serialize)]
pub struct AggregatedBalance {
    pub balance: BigDecimal,
}
