// use super::httpapi::{self, *};
// use super::types::{
//     Block, BlockInfo, Event, EventData, Filter, GetBlockResponse, Log, NoCustom, ParsedLog,
//     Receipt, ToEventData, TransferEvent, TransferValue, TronAddress,
// };

// use crate::common::{Action, ChainInfo, TransactionStatus};

// use crate::harvester::{
//     BlockInfo as BlockInfoT, BlockProcessor, Client as ClientT, Projection,
//     StorageHandle as StorageHandleT,
// };
use crate::bitcoin::types::BlockInfo;
use crate::sqlx_postgres::bitcoin::{self as db};

use std::collections::{HashMap, HashSet};
use std::convert::{Into, TryInto};
use std::fmt::Debug;
use std::iter::FromIterator;
use std::sync::Arc;

use async_trait::async_trait;
use atb_cli::once_cell::sync::OnceCell;
use atb_types::Utc;
use bitcoin::block::Bip34Error;
use bitcoincore_rpc::bitcoin::Block;
use bitcoincore_rpc::{Auth, Client as RpcClient, Error as RpcError};
use itertools::Itertools;
use lazy_static::__Deref;
use num_traits::Zero;
use serde::Serialize;
use sqlx::{PgConnection, PgPool};

// pub type Handle = crate::harvester::Handle<Processor, StorageHandle<DefaultProvider>>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Rpc error: `{0}`")]
    Rpc(#[from] RpcError),

    #[error("Sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Account Balance Overflow")]
    BalanceOverflow,

    #[error("Account Balance Underflow")]
    BalanceUnderflow,

    #[error("BIP34 Error: {0}")]
    Bip34(#[from] Bip34Error),

    /// Other Error
    #[error("Other Error: {0}")]
    Other(Box<dyn std::error::Error + Send + Sync + 'static>),
}
pub struct Client {
    name: String,
    inner: Arc<RpcClient>,
}

impl Client {
    pub fn new(
        name: String,
        url: &str,
        rpc_user: Option<&str>,
        rpc_pwd: Option<&str>,
    ) -> Result<Self, Error> {
        let auth = match (rpc_user, rpc_pwd) {
            (Some(user), Some(pwd)) => Auth::UserPass(user.to_string(), pwd.to_string()),
            _ => Auth::None,
        };
        Ok(Self {
            name,
            inner: Arc::new(RpcClient::new(url, auth)?),
        })
    }

    async fn process_bitcoin_block_to_block_info(
        &self,
        block: Block,
    ) -> Result<Option<BlockInfo>, Error> {
        if block.txdata.is_empty() {
            return Ok(Some(BlockInfo {
                hash: block.block_hash(),
                parent_hash: block.header.prev_blockhash,
                number: block.bip34_block_height()?,
                time: block.header.time,
                transactions: Vec::new(),
            }));
        }

        let mut block_info = BlockInfo {
            hash: block.block_hash(),
            parent_hash: block.header.prev_blockhash,
            number: block.bip34_block_height()?,
            time: block.header.time,
            transactions: Vec::new(),
        };

        for tx in block.txdata {
            // Process Bitcoin transactions
            // Add your Bitcoin-specific transaction processing logic here
            block_info.transactions.push(tx);
        }

        Ok(Some(block_info))
    }

    pub fn inner(&self) -> &RpcClient {
        &self.inner
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
}

impl Processor {
    pub async fn new(conn: PgPool, provider: Arc<Client>) -> Result<Self, Error> {
        let current_sequence = db::get_latest_sequence_id(&conn).await? + 1;

        Ok(Self {
            conn,
            provider,
            current_sequence,
        })
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

// #[derive(Clone)]
// pub struct StorageHandle {
//     conn: PgPool,
//     provider: Arc<Client>,
// }

// #[async_trait]
// impl<T> StorageHandleT for StorageHandle<T>
// where
//     T: TRC20Meta + Send + Sync + 'static,
// {
//     type Error = Error;
//     type Address = Address;

//     async fn commit_addresses(
//         &self,
//         addresses: impl Iterator<Item = &'async_trait (Address, Option<String>)> + Send + 'async_trait,
//     ) -> Result<(), Error> {
//         let mut tx = self.conn.0.begin().await?;

//         for addr in addresses {
//             db::insert_address(&mut tx, &addr.0, addr.1.as_deref()).await?;
//         }

//         tx.commit().await.map_err(Into::into)
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
