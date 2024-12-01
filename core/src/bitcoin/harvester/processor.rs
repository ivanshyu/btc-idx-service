use super::client::Client;
use crate::bitcoin::types::{Action, AggregatorMsg, BlockInfo, BtcP2trEvent, BtcUtxoInfo};
use crate::sqlx_postgres::bitcoin::{self as db};
use crate::{rpc_client, HandlerStorage};

use std::collections::HashMap;
use std::convert::Into;
use std::fmt::Debug;
use std::sync::Arc;

use atb_types::Utc;
use bigdecimal::BigDecimal;
use bitcoin::address::{FromScriptError, NetworkChecked, ParseError};
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

    #[error("Address parse error: {0}")]
    AddressParse(#[from] ParseError),

    #[error("Send event to aggregator error: {0}")]
    SendEvent(#[from] tokio::sync::mpsc::error::SendError<AggregatorMsg>),

    /// Other Error
    #[error("Other Error: {0}")]
    Other(Box<dyn std::error::Error + Send + Sync + 'static>),

    /// Anyhow Error
    #[error("Anyhow Error: {0}")]
    Anyhow(#[from] anyhow::Error),
}

pub struct Processor {
    conn: PgPool,
    provider: Arc<Client>,
    network: Network,
    event_sender: UnboundedSender<AggregatorMsg>,
}

impl Processor {
    pub async fn new(
        conn: PgPool,
        provider: Arc<Client>,
        network: Network,
        event_sender: UnboundedSender<AggregatorMsg>,
    ) -> Result<Self, Error> {
        Ok(Self {
            conn,
            provider,
            network,
            event_sender,
        })
    }

    pub fn get_network(&self) -> Network {
        self.network
    }

    pub async fn is_processed(&self, block_hash: &BlockHash) -> Result<bool, Error> {
        db::has_block(&self.conn, block_hash)
            .await
            .map_err(Into::into)
    }

    pub async fn process(&mut self, block: &BlockInfo) -> Result<(), Error> {
        let mut db_tx = self.conn.begin().await?;

        let block_num = block.header.height;
        // log::info!("Processing block {:?}", &block);
        // TODO: check the block is confirmed
        db::upsert_block(
            &mut *db_tx,
            &block.header.hash,
            block.header.previous_block_hash.as_ref(),
            block_num,
            Utc.timestamp_opt(block.header.time as i64, 0)
                .single()
                .ok_or_else(|| anyhow::anyhow!("Invalid timestamp"))?,
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
            let txid = block_tx.compute_txid();

            // utxo owner -> balance changes
            let mut balance_updates: HashMap<Address<NetworkChecked>, BigDecimal> = HashMap::new();

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

                let address = Address::from_script(script, self.network)?;

                let amount = BigDecimal::from(txout.value.to_sat());

                let balance = match balance_updates.get(&address) {
                    Some(b) => b.clone(),
                    None => BigDecimal::zero(),
                };

                db::create_utxo(
                    &mut *db_tx,
                    &address.to_string(),
                    txid,
                    idx,
                    &txout.value.to_sat().into(),
                    block_num,
                )
                .await?;

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

                let event = BtcP2trEvent::new(
                    block_num,
                    txid,
                    address,
                    value.clone(),
                    action,
                    block_tx.is_coinbase(),
                );

                self.event_sender.send(AggregatorMsg::Event(event)).unwrap();
            }
        }

        db_tx.commit().await?;

        Ok(())
    }

    pub fn db_connection(&self) -> HandlerStorage {
        HandlerStorage {
            conn: self.conn.clone(),
        }
    }
}
