use bigdecimal::BigDecimal;
use bitcoin::{address::NetworkChecked, hash_types::Txid, Address, Block, Network, OutPoint};
use bitcoincore_rpc::json::GetBlockHeaderResult;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

pub static BTC_NETWORK: OnceCell<Network> = OnceCell::new();

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct BtcBlock {
    pub block_hash: String,
    pub block_number: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct BtcP2trEvent {
    pub sequence_id: i64,
    pub block_number: usize,
    pub txid: Txid,
    pub address: String,
    pub amount: BigDecimal,
    pub action: Action,
}

impl BtcP2trEvent {
    pub fn new(
        block_number: usize,
        txid: Txid,
        address: String,
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
