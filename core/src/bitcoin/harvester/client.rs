use crate::bitcoin::types::{Action, BlockInfo, BtcP2trEvent, BtcUtxoInfo};
use crate::rpc_client::{self, BitcoinRpcClient};
use crate::sqlx_postgres::bitcoin::{self as db};
use crate::HandlerStorage;

use std::collections::HashMap;
use std::convert::Into;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;

use atb_types::Utc;
use bigdecimal::BigDecimal;
use bitcoin::address::FromScriptError;
use bitcoin::blockdata::transaction::OutPoint;
use bitcoin::{Address, BlockHash, Network};
use num_traits::{FromPrimitive, Zero};
use sqlx::types::chrono::TimeZone;
use sqlx::PgPool;
use tokio::sync::mpsc::UnboundedSender;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Rpc error: `{0}`")]
    Rpc(#[from] rpc_client::Error),

    #[error("Sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Account Balance Overflow")]
    BalanceOverflow,

    #[error("Account Balance Underflow")]
    BalanceUnderflow,

    #[error("Address from script error: {0}")]
    AddressFromScript(#[from] FromScriptError),

    #[error("Send event to aggregator error: {0}")]
    SendEvent(#[from] tokio::sync::mpsc::error::SendError<BtcP2trEvent>),

    /// Other Error
    #[error("Other Error: {0}")]
    Other(Box<dyn std::error::Error + Send + Sync + 'static>),
}
pub struct Client {
    name: String,
    inner: Arc<BitcoinRpcClient>,
}

impl Client {
    pub fn new(
        name: String,
        url: String,
        rpc_user: Option<String>,
        rpc_pwd: Option<String>,
    ) -> Self {
        log::info!(
            "Connecting to Bitcoin node: {}, user: {:?}/{:?}",
            url,
            rpc_user,
            rpc_pwd
        );

        let rpc_client = BitcoinRpcClient::new(url, rpc_user, rpc_pwd);
        Self {
            name,
            inner: Arc::new(rpc_client),
        }
    }

    // async fn (
    //     &self,
    //     block: GetBlockResult,
    // ) -> Result<Option<BlockInfo>, Error> {
    //     let mut block_info = BlockInfo {
    //         hash: block.hash,
    //         header: Header {
    //             version: Version::from_consensus(block.version),
    //             prev_blockhash: block.previousblockhash.unwrap_or_default(),
    //             merkle_root: block.merkleroot,
    //             time: block.time as u32,
    //             bits: CompactTarget::from_hex(&block.bits),
    //             nonce: block.nonce,
    //         },
    //         number: block.height,
    //         transactions: Vec::new(),
    //     };

    //     if block.tx.is_empty() {
    //         return Ok(Some(BlockInfo {
    //             hash: block.hash,
    //             header: block.header,
    //             number: block.height,
    //             transactions: Vec::new(),
    //         }));
    //     }

    //     for tx in block.txdata {
    //         // Process Bitcoin transactions
    //         // Add your Bitcoin-specific transaction processing logic here
    //         block_info.transactions.push(tx);
    //     }

    //     Ok(Some(block_info))
    // }

    pub fn inner(&self) -> &BitcoinRpcClient {
        &self.inner
    }

    pub async fn get_tip_number(&self) -> Result<usize, Error> {
        (&*self.inner).get_block_count().await.map_err(Into::into)
    }

    // TODO: future join
    pub async fn scan_block(&self, number: Option<usize>) -> Result<BlockInfo, Error> {
        let (header, block) = match number {
            Some(num) => {
                let hash = self.inner.get_block_hash(num).await?;

                (
                    self.inner.get_block_header(&hash).await?,
                    self.inner.get_block(&hash).await?,
                )
            }
            _ => {
                let num = self.inner.get_block_count().await?;
                let hash = self.inner.get_block_hash(num).await?;

                (
                    self.inner.get_block_header(&hash).await?,
                    self.inner.get_block(&hash).await?,
                )
            }
        };
        Ok(BlockInfo {
            header,
            body: block,
        })
    }
}

impl Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").field("name", &self.name).finish()
    }
}

pub struct Processor {
    conn: PgPool,
    provider: Arc<Client>,
    current_sequence: i64,
    network: Network,
    event_sender: UnboundedSender<BtcP2trEvent>,
}

impl Processor {
    pub async fn new(
        conn: PgPool,
        provider: Arc<Client>,
        network: Network,
        event_sender: UnboundedSender<BtcP2trEvent>,
    ) -> Result<Self, Error> {
        // let current_sequence = db::get_latest_sequence_id(&conn).await? + 1;
        let current_sequence = 0;
        Ok(Self {
            conn,
            provider,
            current_sequence,
            network,
            event_sender,
        })
    }

    pub fn get_network(&self) -> Network {
        self.network
    }

    async fn is_processed(&self, block_hash: &BlockHash) -> Result<bool, Error> {
        db::has_block(&self.conn, block_hash)
            .await
            .map_err(Into::into)
    }

    pub async fn process(&mut self, block: &BlockInfo) -> Result<(), Error> {
        let mut db_tx = self.conn.begin().await?;

        let mut sequence_id = self.current_sequence;
        let block_num = block.header.height;
        // log::info!("Processing block {:?}", &block);
        // TODO: check the block is confirmed
        db::upsert_block(
            &mut *db_tx,
            &block.header.hash,
            block.header.previous_block_hash.as_ref(),
            block_num,
            Utc.timestamp(block.header.time as i64, 0),
            &BigDecimal::from(block.header.nonce),
            block.header.version.to_consensus(),
            &BigDecimal::from_f64(block.header.difficulty).unwrap_or_default(),
        )
        .await?;

        let mut outpoints = Vec::new();

        for (idx, tx) in block.body.txdata.iter().enumerate() {
            db::upsert_transaction(
                &mut *db_tx,
                &tx.compute_txid(),
                &block.header.hash,
                idx as i32,
                &BigDecimal::from(tx.lock_time.to_consensus_u32()),
                tx.is_coinbase(),
                tx.version.0,
            )
            .await?;
            // First pass, get all vins
            for txin in &tx.input {
                outpoints.push(&txin.previous_output);
            }
        }

        let mut relevant_utxos: HashMap<OutPoint, BtcUtxoInfo> =
            db::get_relevant_utxos(&mut *db_tx, &outpoints)
                .await?
                .into_iter()
                .map(|info| (info.get_out_point(), info))
                .collect();

        for block_tx in &block.body.txdata {
            let txid = block_tx.txid();

            // utxo owner -> balance changes
            let mut balance_updates: HashMap<String, BigDecimal> = HashMap::new();

            // Handle TxIns, deduct previos utxo amount, and mark them as used
            for txin in &block_tx.input {
                let utxo = match relevant_utxos.remove(&txin.previous_output) {
                    None => continue,
                    Some(utxo) => utxo,
                };

                // if execute && rollback_spent_utxos.remove(&txin.previous_output).is_none()
                db::spend_utxo(&mut *db_tx, utxo.txid, utxo.vout, block_num).await?;

                if let Some(balance) = balance_updates.get_mut(&utxo.owner) {
                    *balance -= &utxo.amount;
                }

                // balance change will be negative
                let balance = -&utxo.amount
                    + match balance_updates.get(&utxo.owner) {
                        Some(b) => b.clone(),
                        None => BigDecimal::zero(),
                    };

                balance_updates.insert(utxo.owner, balance);
            }

            // Handle TxOuts, add balance if the address is taproot
            for (idx, txout) in block_tx.output.iter().enumerate() {
                let script = &txout.script_pubkey.as_script();

                // only handle p2tr balance
                if !script.is_p2tr() {
                    continue;
                }

                let recipient = Address::from_script(script, self.network)?;
                let address = recipient.to_string();

                let amount = BigDecimal::from(txout.value.to_sat());

                let balance = match balance_updates.get(&address) {
                    Some(b) => b.clone(),
                    None => BigDecimal::zero(),
                };

                let out_point = OutPoint {
                    txid,
                    vout: idx as u32,
                };

                // if execute && rollback_utxos.remove(&out_point).is_none() {
                db::create_utxo(
                    &mut *db_tx,
                    &address,
                    txid,
                    idx,
                    &txout.value.to_sat().into(),
                    block_num,
                )
                .await?;
                // }

                let balance = balance + amount;
                balance_updates.insert(address, balance);
            }

            // Create p2tr events and update balances as needed
            for (address, value) in balance_updates {
                let action = match value.clone() {
                    val if val.is_zero() => continue,
                    val if val > BigDecimal::zero() => Action::Receive,
                    _ => Action::Send,
                };

                let event =
                    BtcP2trEvent::new(block_num, txid, address.clone(), value.clone(), action);

                // if let Some(old_event) = rollback_events.remove(&event.get_key()) {
                //     if event.block_number != old_event.block_number {
                //         event.action = Action::Reorg;
                //         wallet_event_queue.push(event);
                //     }
                // } else {
                //     wallet_event_queue.push(event);

                //     if execute {
                //         db::increment_btc_balance(&mut *db_tx, &address, &value, Utc::now())
                //             .await?;
                //     }
                // }
                self.event_sender.send(event)?;
            }
        }

        db_tx.commit().await?;
        self.current_sequence = sequence_id;

        Ok(())
    }

    pub fn db_connection(&self) -> HandlerStorage {
        HandlerStorage {
            conn: self.conn.clone(),
        }
    }
}
