use crate::bitcoin::types::{Action, BlockInfo, BtcP2trEvent, BtcUtxoInfo};
use crate::rpc_client::{self, BitcoinRpcClient};
use crate::sqlx_postgres::bitcoin::{self as db};
use crate::HandlerStorage;

use std::collections::HashMap;
use std::convert::Into;
use std::fmt::Debug;
use std::sync::Arc;

use atb_types::Utc;
use bigdecimal::BigDecimal;
use bitcoin::address::FromScriptError;
use bitcoin::blockdata::transaction::OutPoint;
use bitcoin::{Address, BlockHash, Network};
use num_traits::{FromPrimitive, Zero};
use sqlx::types::chrono::TimeZone;
use sqlx::PgPool;

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
}

impl Processor {
    pub async fn new(conn: PgPool, provider: Arc<Client>, network: Network) -> Result<Self, Error> {
        // let current_sequence = db::get_latest_sequence_id(&conn).await? + 1;
        let current_sequence = 0;
        Ok(Self {
            conn,
            provider,
            current_sequence,
            network,
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

            // Handle TxIns
            for txin in &block_tx.input {
                let utxo = match relevant_utxos.remove(&txin.previous_output) {
                    None => continue,
                    Some(utxo) => utxo,
                };

                // balance change will be negative
                let balance = -&utxo.amount
                    + match balance_updates.get(&utxo.owner) {
                        Some(b) => b.clone(),
                        None => BigDecimal::zero(),
                    };

                // if execute && rollback_spent_utxos.remove(&txin.previous_output).is_none()
                db::spend_utxo(&mut *db_tx, utxo.txid, utxo.vout, block_num).await?;

                balance_updates.insert(utxo.owner, balance);
            }

            // Handle TxOuts
            for (idx, txout) in block_tx.output.iter().enumerate() {
                let script = &txout.script_pubkey.as_script();

                // only handle p2tr balance
                if !script.is_p2tr() {
                    continue;
                }

                let recipient = Address::from_script(script, self.network)?;
                let address = recipient.to_string();

                let amount = BigDecimal::from(txout.value.to_sat());

                let address = recipient.to_string();

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

                let mut event =
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
// #[async_trait]
// impl<E> BlockProcessor for Processor<E>
// where
//     E: Projection<Error = Error, Store = PgConnection>
//         + ToEventData<EventDataKind = E>
//         + Serialize
//         + serde::de::DeserializeOwned
//         + std::hash::Hash
//         + Unpin
//         + Eq
//         + Clone
//         + Send
//         + Sync
//         + 'static,
// {
//     type Error = Error;

//     type Event = Event<E>;
//     type Block = BlockInfo;
//     type BlockHash = Hash;
//     type BlockNumber = u64;
//     type Address = Address;
//     type StorageHandle = StorageHandle<T>;

//     async fn is_processed(&self, block_hash: &Hash) -> Result<bool, Error> {
//         self.conn.has_block(block_hash).await.map_err(Into::into)
//     }

//     async fn process(
//         &mut self,
//         addresses: &HashMap<Address, Option<String>>,
//         block: &BlockInfo,
//     ) -> Result<Vec<Self::Event>, Error> {
//         let mut events_to_process = Vec::new();
//         let mut tx = self.conn.begin().await?;

//         Self::extract_events(
//             &mut tx,
//             block,
//             addresses,
//             &self.trx_dispatch_contract,
//             &mut events_to_process,
//         )
//         .await?;

//         let mut sequence_id = self.current_sequence;
//         for event in events_to_process.iter_mut() {
//             assert!(
//                 !event.orphaned,
//                 "orphanned events should not be generated during processing"
//             );
//             event.project(&mut tx, self.provider.deref()).await?;
//             event.sequence_id = sequence_id;
//             sequence_id += 1;
//         }

//         db::create_events(&mut tx, &events_to_process).await?;
//         tx.commit().await?;
//         self.current_sequence = sequence_id;
//         Ok(events_to_process)
//     }

//     async fn reprocess(
//         &mut self,
//         addresses: &HashMap<Address, Option<String>>,
//         block: &BlockInfo,
//     ) -> Result<Vec<Self::Event>, Error> {
//         let mut events_to_process = Vec::new();
//         let mut processed_events = HashSet::new();
//         let mut tx = self.conn.0.begin().await?;

//         // Remove old blocks and get corresponding TXs
//         db::get_events_at_block(&mut tx, block.number())
//             .await?
//             .into_iter()
//             .for_each(|e| {
//                 // Cancel Out Previous Action / OrphanedAction with the same TX Hash + Action
//                 // using the Event's Hash implementation
//                 if !processed_events.remove(&e) {
//                     processed_events.insert(e);
//                 }
//             });

//         // Process txs in new blocks and unmark duplicate ones
//         Self::extract_events(
//             &mut tx,
//             block,
//             addresses,
//             &self.trx_dispatch_contract,
//             &mut events_to_process,
//         )
//         .await?;

//         let mut events_to_process = Self::organize_events(events_to_process, processed_events);
//         let mut sequence_id = self.current_sequence;
//         for event in events_to_process.iter_mut() {
//             assert!(
//                 !event.orphaned,
//                 "orphanned events should not be generated during reprocessing"
//             );
//             event.project(&mut tx, self.provider.deref()).await?;
//             event.sequence_id = sequence_id;
//             sequence_id += 1;
//         }

//         db::create_events(&mut tx, &events_to_process).await?;
//         tx.commit().await?;
//         self.current_sequence = sequence_id;
//         Ok(events_to_process)
//     }

//     async fn process_fork(
//         &mut self,
//         addresses: &HashMap<Address, Option<String>>,
//         mut blocks: Vec<BlockInfo>,
//     ) -> Result<Vec<Self::Event>, Self::Error> {
//         if blocks.is_empty() {
//             log::warn!("empty block array given for fork processing");
//             return Ok(Vec::new());
//         }

//         let mut events_to_process = Vec::new();
//         let mut processed_events = HashSet::new();
//         let mut tx = self.conn.0.begin().await?;
//         let begin_block_num = blocks.last().expect("checked not empty").number();

//         // Remove old blocks and get corresponding TXs
//         db::remove_blocks_since(&mut tx, begin_block_num).await?;
//         db::get_events_since_block(&mut tx, begin_block_num)
//             .await?
//             .into_iter()
//             .for_each(|e| {
//                 // Cancel Out Previous Action / OrphanedAction with the same TX Hash + Action
//                 // using the Event's Hash implementation
//                 if !processed_events.remove(&e) {
//                     processed_events.insert(e);
//                 }
//             });

//         // Process txs in new blocks and unmark duplicate ones
//         blocks.reverse();
//         for block in blocks {
//             Self::extract_events(
//                 &mut tx,
//                 &block,
//                 addresses,
//                 &self.trx_dispatch_contract,
//                 &mut events_to_process,
//             )
//             .await?;
//         }

//         let mut events_to_process = Self::organize_events(events_to_process, processed_events);
//         let mut sequence_id = self.current_sequence;
//         for event in events_to_process.iter_mut() {
//             if event.orphaned {
//                 event.unproject(&mut tx).await?;
//             } else {
//                 event.project(&mut tx, self.provider.deref()).await?;
//             }
//             event.sequence_id = sequence_id;
//             sequence_id += 1;
//         }

//         db::create_events(&mut tx, &events_to_process).await?;
//         tx.commit().await?;
//         self.current_sequence = sequence_id;
//         Ok(events_to_process)
//     }

//     async fn get_newest_processed_block(&self) -> Result<Option<Self::Block>, Self::Error> {
//         self.conn
//             .get_newest_block()
//             .await
//             .map(|b| b.map(BlockInfo::new_dummy))
//             .map_err(Into::into)
//     }

//     async fn log_activity<D: Serialize + Send + Sync + 'static>(
//         &self,
//         kind: &str,
//         data: Option<D>,
//     ) -> Result<(), Self::Error> {
//         db::insert_activity_log(&self.conn.0, kind, Utc::now(), data)
//             .await
//             .map_err(Into::into)
//     }

//     fn storage_handle(&self) -> Self::StorageHandle {
//         StorageHandle {
//             conn: self.conn.clone(),
//             provider: self.provider.clone(),
//         }
//     }
// }

// #[cfg(test)]
// mod test {
//     use super::*;

//     use std::str::FromStr;

//     use atb::logging::init_logger;
//     use sqlx::postgres::PgPoolOptions;
//     use sqlx::{Connection, PgPool};

//     type Harvester = crate::harvester::Harvester<Processor<Provider>, Client>;

//     async fn setup_harvester(pg_pool: sqlx::PgPool, start_block: Option<u64>) -> Harvester {
//         // const NODE_URL: &'static str = "http://localhost:9090";
//         const NODE_URL: &str = "https://api.shasta.trongrid.io";
//         // let mut address_book = HashSet::<H160, _>::new();

//         // address_book.insert(
//         //     TronAddress::from_str("TXvdw1KJQpaKinFZ8hthUu6aG9kKshynDK")
//         //         .unwrap()
//         //         .into(),
//         // );

//         let client = Client::new(
//             "Tron".to_owned(),
//             NODE_URL,
//             None,
//             None,
//             HashMap::new(),
//             HashSet::default(),
//             None,
//             vec![],
//         )
//         .expect("should create client");
//         let block_processor = Processor::<Provider>::new(pg_pool.into(), NODE_URL.into(), None)
//             .await
//             .expect("should create processor");

//         Harvester::new(
//             client,
//             block_processor,
//             start_block,
//             0,
//             3000,
//             "TRON_TEST".to_owned(),
//         )
//         .await
//         .expect("should create harvester")
//     }

//     async fn connect_postgres() -> (PgConnection, PgPool) {
//         let database_url = "postgres://postgres:123456@localhost:5432/tron?sslmode=disable";
//         let conn = PgConnection::connect(database_url)
//             .await
//             .expect("postgres available");

//         let pool = PgPoolOptions::new()
//             .max_connections(5)
//             .connect(database_url)
//             .await
//             .expect("postgres available");
//         (conn, pool)
//     }

//     #[test]
//     fn test_all() {}

//     #[test]
//     fn test_parse_log() {}
// }
