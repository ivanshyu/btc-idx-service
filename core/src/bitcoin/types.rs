use atb_types::DateTime;
use bigdecimal::BigDecimal;
use bitcoin::{
    address::NetworkChecked, hash_types::Txid, Address, Block, BlockHash, Network, OutPoint,
};
use bitcoincore_rpc::json::GetBlockHeaderResult;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

pub static BTC_NETWORK: OnceCell<Network> = OnceCell::new();

#[derive(Clone, Debug)]
pub struct BtcBlock {
    pub hash: BlockHash,
    pub number: usize,
    pub previous_hash: BlockHash,
    pub timestamp: DateTime,
    pub nonce: i64,
    pub version: i32,
    pub difficulty: BigDecimal,
}
#[derive(Clone, Debug)]
pub struct BtcTransaction {
    pub txid: Txid,
    pub block_hash: BlockHash,
    pub transaction_index: usize,
    pub lock_time: u64,
    pub version: i32,
}

#[derive(Clone, Debug)]
pub struct BtcBalance {
    pub address: Address<NetworkChecked>,
    pub amount: BigDecimal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct BtcUtxo {
    pub txid: Txid,
    pub vout: u32,
    pub amount: BigDecimal,
}

#[derive(Clone, Debug)]
pub struct BtcUtxoInfo {
    pub owner: Address<NetworkChecked>,
    pub txid: Txid,
    pub vout: u32,
    pub amount: BigDecimal,
    pub block_number: u64,
    pub spent_block: Option<u64>,
}

pub type BtcUtxoInfoKey = (Txid, u32);

impl BtcUtxoInfo {
    pub fn get_out_point(&self) -> OutPoint {
        OutPoint {
            txid: self.txid,
            vout: self.vout,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BtcP2trEvent {
    pub sequence_id: i64,
    pub block_number: usize,
    pub txid: Txid,
    pub address: Address<NetworkChecked>,
    pub amount: BigDecimal,
    pub action: Action,
}

impl BtcP2trEvent {
    pub fn new(
        block_number: usize,
        txid: Txid,
        address: Address<NetworkChecked>,
        amount: BigDecimal,
        action: Action,
    ) -> Self {
        Self {
            sequence_id: Default::default(),
            block_number,
            txid,
            address,
            amount,
            action,
        }
    }
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum Action {
    Send = 0,
    Receive = 1,
}

impl TryFrom<i16> for Action {
    type Error = anyhow::Error;

    fn try_from(v: i16) -> Result<Self, Self::Error> {
        match v {
            x if x == Action::Receive as i16 => Ok(Action::Receive),
            x if x == Action::Send as i16 => Ok(Action::Send),
            _ => Err(anyhow::anyhow!("invalid action type: {}", v)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct BlockInfo {
    pub header: GetBlockHeaderResult,
    pub body: Block,
}
