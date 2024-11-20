use bigdecimal::BigDecimal;
use bitcoin::{address::NetworkUnchecked, hash_types::Txid, Address, OutPoint};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct BtcBlock {
    pub block_hash: String,
    pub block_number: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct BtcBalance {
    pub address: Address<NetworkUnchecked>,
    pub amount: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct BtcUtxo {
    pub txid: Txid,
    pub vout: u32,
    pub amount: String,
}

#[derive(Clone, Debug)]
pub struct BtcUtxoInfo {
    pub owner: String,
    pub txid: Txid,
    pub vout: u32,
    pub amount: BigDecimal,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct BtcWalletEvent {
    pub sequence_id: i64,
    pub block_number: u64,
    pub tx_hash: String,
    pub address: Address<NetworkUnchecked>,
    pub amount: String,
    pub action: Action,
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

pub struct BlockInfo {
    pub hash: bitcoin::BlockHash,
    pub parent_hash: bitcoin::BlockHash,
    pub number: u64,
    pub time: u32,
    pub transactions: Vec<bitcoin::Transaction>,
}
