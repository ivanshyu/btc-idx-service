use std::collections::HashMap;

use atb_cli::DateTime;
use bitcoin::{BlockHash, Txid};
use serde::{Deserialize, Serialize};
use sqlx::types::BigDecimal;

#[derive(Serialize)]
pub struct LatestBlockHeight {
    pub latest_block_height: usize,
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
    pub version: i64,
    pub difficulty: f64,
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
    pub version: i64,
}

#[derive(Serialize)]
pub struct CurrentBalance {
    pub current_balance: BigDecimal,
}

#[derive(Serialize)]
pub struct Utxo {
    pub transaction_id: Txid,
    pub transaction_index: usize,
    pub satoshi: BigDecimal,
    pub block_height: usize,
}

pub type AggregatedInfo = HashMap<DateTime, Vec<AggregatedBalance>>;

#[derive(Serialize)]
pub struct AggregatedBalance {
    pub balance: BigDecimal,
}
