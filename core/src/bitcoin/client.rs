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

    #[error("ParseLog error: `{0}`")]
    ParseLog(String),

    /// Sqlx Error
    #[error("Sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Account Balance Overflow")]
    BalanceOverflow,

    #[error("Account Balance Underflow")]
    BalanceUnderflow,

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
                number: block.height.unwrap_or(0),
                time: block.header.time,
                transactions: Vec::new(),
            }));
        }

        let mut transactions = Vec::new();
        for tx in block.txdata {
            // Process Bitcoin transactions
            // Add your Bitcoin-specific transaction processing logic here
            transactions.push(tx);
        }

        Ok(Some(BlockInfo {
            hash: block.block_hash(),
            parent_hash: block.header.prev_blockhash,
            number: block.height.unwrap_or(0),
            time: block.header.time,
            transactions,
        }))
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

#[async_trait]
impl ClientT for Client {
    type Error = Error;
    type Block = BlockInfo;
    type BlockHash = bitcoin::BlockHash;
    type BlockNumber = u64;
    type Address = bitcoin::Address;

    async fn get_tip_number(&self) -> Result<Self::BlockNumber, Self::Error> {
        Ok(self.inner.get_block_count()? as u64)
    }

    async fn mine_block_by_hash(
        &mut self,
        hash: &Self::BlockHash,
    ) -> Result<Option<Self::Block>, Self::Error> {
        let block = self.inner.get_block(hash)?;
        self.process_bitcoin_block_to_block_info(block).await
    }

    async fn mine_block(
        &mut self,
        number: Option<Self::BlockNumber>,
    ) -> Result<Option<Self::Block>, Self::Error> {
        let block = match number {
            Some(num) => {
                let hash = self.inner.get_block_hash(num as u64)?;
                self.inner.get_block(&hash)?
            }
            None => {
                let hash = self.inner.get_best_block_hash()?;
                self.inner.get_block(&hash)?
            }
        };

        self.process_bitcoin_block_to_block_info(block).await
    }

    async fn set_environment(
        &self,
        pool: &PgPool,
        global: &OnceCell<ChainInfo>,
        chain_id: u64,
        chain_type: String,
        mainnet: bool,
    ) -> Result<(), Self::Error> {
        let chain_info = ChainInfo {
            chain_id,
            chain_type,
            mainnet,
        };

        if let Some(info) = get_config::<_, ChainInfo>(pool, "chain_info").await? {
            if chain_info != info {
                return Err(Error::Other(
                    "Client: database has different chain info".into(),
                ));
            }
        }

        upsert_config(pool, "chain_info", &chain_info).await?;
        log::debug!("{:?}", chain_info);
        global
            .set(chain_info)
            .expect("Client: global ChainInfo should only be set once.");

        Ok(())
    }
}

pub struct Processor {
    conn: PgPool,
    provider: Arc<Client>,
    current_sequence: i64,
}

impl Processor {
    pub async fn new(conn: PgPool, provider: Arc<Client>) -> Result<Self, Error> {
        let current_sequence = conn.get_latest_sequence_id().await? + 1;

        Ok(Self {
            conn,
            provider: Arc::new(provider),
            current_sequence,
        })
    }

    // Find events that are New
    // - filter all events that already exist inside `processed_events` (nothing changed)
    // - remove from `processed_events`, only leaving orphans
    // - events to process is left with only new events detected on fork
    fn organize_events(
        events: Vec<Event<E>>,
        mut processed_events: HashSet<Event<E>>,
    ) -> Vec<Event<E>> {
        let events = events
            .into_iter()
            .filter(|e| !processed_events.remove(e))
            .collect::<Vec<Event<E>>>();

        // Mark events as orphaned and add to events to process
        // - sort is required since processed_events are orphans from a HashMap (non-ordered)
        // - #NOTE orphans are first in the list.  The order becomes [`old`, `orphans`, `new`]
        Vec::from_iter(processed_events)
            .into_iter()
            .sorted_by(|a, b| b.sequence_id.partial_cmp(&a.sequence_id).unwrap())
            .map(|mut e| {
                e.orphaned = true;
                e
            })
            .chain(events)
            .collect()
    }

    // #[instrument(skip_all, fields(block_num))]
    async fn extract_events(
        conn: &mut PgConnection,
        block: &BlockInfo,
        addresses: &HashMap<Address, Option<String>>,
        trx_dispatch_contract: &Option<Address>,
        events_to_process: &mut Vec<Event<E>>,
    ) -> Result<(), Error> {
        let block_number = block.number();
        let block_hash = &block.hash();
        // tracing::Span::current().record("block_num", &block_number);

        log::trace!("Processing block {}, hash: {:?}", block_number, block_hash);
        // Insert block
        db::create_block(conn, block_hash, block_number, block.time()).await?;

        // handle native receives, gas data omitted
        for tx_hash in &block.receive_transactions {
            let tx = block.txs.get(tx_hash).expect("tx should exist");

            let primary_address: TronAddress = tx.get_to_address().unwrap_or_else(|| {
                tx.get_contract_address()
                    .expect("should get send_tx get_contract_address when to_address is not found")
            });
            let correlation_id = addresses
                .get(&primary_address.clone().into())
                .cloned()
                .expect("event primary address should be in addresses");
            let event = Event {
                sequence_id: 0,
                block_number,
                tx_hash: tx.tx_id,
                orphaned: false,
                data: EventData::Standard(TransferEvent {
                    primary_address,
                    correlation_id,
                    secondary_address: tx
                        .get_from_address()
                        .expect("should get send_tx primary_address"),
                    value: tx.get_amount().unwrap_or_default(),
                    bandwidth_usage: 0,
                    bandwidth_fee: 0,
                    energy_usage: 0,
                    origin_energy_usage: 0,
                    energy_fee: 0,
                    token_address: None,
                    token_value: None,
                    action: Action::Receive,
                    extension: tx.get_data(),
                }),
            };

            events_to_process.push(event);
        }

        // handle native and failed x20 sends tx gas calculated
        for tx_hash in &block.send_transactions {
            let tx = block.txs.get(tx_hash).expect("tx should exist");
            let receipt = block.receipts.get(tx_hash).expect("tx should exist");

            // filter out the failed tx value, only count the gas fee
            let value = if tx.is_success() {
                tx.get_amount().unwrap_or_default()
            } else {
                0u64
            };
            let primary_address: TronAddress = tx
                .get_from_address()
                .expect("should get send_tx primary_address");
            let correlation_id = addresses
                .get(&primary_address.clone().into())
                .cloned()
                .expect("event primary address should be in addresses");

            // Note this will be native transfer or sendMany
            let event = Event {
                sequence_id: 0,
                block_number,
                tx_hash: tx.tx_id,
                orphaned: false,
                data: EventData::Standard(TransferEvent {
                    primary_address,
                    correlation_id,
                    secondary_address: tx.get_to_address().unwrap_or_else(|| {
                        tx.get_contract_address().expect(
                            "should get send_tx get_contract_address when to_address is not found",
                        )
                    }),
                    value,
                    bandwidth_usage: receipt.bandwidth_usage.unwrap_or_default(),
                    bandwidth_fee: receipt.bandwidth_fee.unwrap_or_default(),
                    energy_usage: receipt.energy_usage.unwrap_or_default(),
                    origin_energy_usage: receipt.origin_energy_usage.unwrap_or_default(),
                    energy_fee: receipt.energy_fee.unwrap_or_default(),
                    token_address: None,
                    token_value: None,
                    action: Action::Send,
                    extension: tx.get_data(),
                }),
            };

            events_to_process.push(event);
        }

        // ERC 20 and Eth Sender Contracts
        // Assume all logs included are relevant
        for (log, parsed_log) in &block.receive_logs {
            let tx_hash = log
                .transaction_hash
                .expect("log transaction hash should exist. qed");

            let tx = block.txs.get(&tx_hash).expect("tx should exist");

            let token_address = match trx_dispatch_contract {
                Some(addr) if addr == &log.address => None,
                _ => Some(TronAddress::Hex(log.address)),
            };
            let token_value = parsed_log
                .value
                .try_into()
                .expect("should not be too large");

            let correlation_id = addresses
                .get(&parsed_log.to)
                .cloned()
                .expect("event primary address should be in addresses");

            let ext_data = ERC20_CONTRACT
                .encode("transfer", (parsed_log.to, token_value))
                .ok()
                .and_then(|calldata| {
                    tx.get_data()
                        .and_then(|d| find_tailing_bytes(&d, &calldata))
                });

            let event = Event {
                sequence_id: 0,
                block_number,
                tx_hash,
                orphaned: false,
                data: EventData::Standard(TransferEvent {
                    primary_address: parsed_log.to.into(),
                    correlation_id,
                    secondary_address: parsed_log.from.into(),
                    value: 0,
                    bandwidth_usage: 0,
                    bandwidth_fee: 0,
                    energy_usage: 0,
                    origin_energy_usage: 0,
                    energy_fee: 0,
                    token_address,
                    token_value: Some(token_value),
                    action: Action::Receive,
                    extension: ext_data,
                }),
            };

            events_to_process.push(event);
        }

        for (log, parsed_log) in &block.send_logs {
            let tx_hash = log
                .transaction_hash
                .expect("log transaction hash should exist. qed");
            let tx = block.txs.get(&tx_hash).expect("tx should exist");
            let receipt = block.receipts.get(&tx_hash).expect("tx should exist");

            let token_address = match trx_dispatch_contract {
                Some(addr) if addr == &log.address => None,
                _ => Some(TronAddress::Hex(log.address)),
            };
            let token_value = parsed_log
                .value
                .try_into()
                .expect("should not be too large");

            let correlation_id = addresses
                .get(&parsed_log.from)
                .cloned()
                .expect("event primary address should be in addresses");

            let ext_data = ERC20_CONTRACT
                .encode("transfer", (parsed_log.to, token_value))
                .ok()
                .and_then(|calldata| {
                    tx.get_data()
                        .and_then(|d| find_tailing_bytes(&d, &calldata))
                });

            let event = Event {
                sequence_id: 0,
                block_number,
                tx_hash,
                orphaned: false,
                data: EventData::Standard(TransferEvent {
                    primary_address: parsed_log.from.into(),
                    correlation_id,
                    secondary_address: parsed_log.to.into(),
                    value: tx.get_amount().unwrap_or_default(),
                    bandwidth_usage: receipt.bandwidth_usage.unwrap_or_default(),
                    bandwidth_fee: receipt.bandwidth_fee.unwrap_or_default(),
                    energy_usage: receipt.energy_usage.unwrap_or_default(),
                    origin_energy_usage: receipt.origin_energy_usage.unwrap_or_default(),
                    energy_fee: receipt.energy_fee.unwrap_or_default(),
                    token_address,
                    token_value: Some(token_value),
                    action: Action::Send,
                    extension: ext_data,
                }),
            };

            events_to_process.push(event);
        }

        for (tx_hash, data) in E::to_event_data(&block.custom_logs) {
            let event = Event {
                sequence_id: 0,
                block_number,
                tx_hash,
                orphaned: false,
                data: EventData::Custom(data),
            };

            events_to_process.push(event);
        }

        Ok(())
    }
}

#[async_trait]
impl<E> BlockProcessor for Processor<E>
where
    E: Projection<Error = Error, Store = PgConnection>
        + ToEventData<EventDataKind = E>
        + Serialize
        + serde::de::DeserializeOwned
        + std::hash::Hash
        + Unpin
        + Eq
        + Clone
        + Send
        + Sync
        + 'static,
{
    type Error = Error;

    type Event = Event<E>;
    type Block = BlockInfo;
    type BlockHash = Hash;
    type BlockNumber = u64;
    type Address = Address;
    type StorageHandle = StorageHandle<T>;

    async fn is_processed(&self, block_hash: &Hash) -> Result<bool, Error> {
        self.conn.has_block(block_hash).await.map_err(Into::into)
    }

    async fn process(
        &mut self,
        addresses: &HashMap<Address, Option<String>>,
        block: &BlockInfo,
    ) -> Result<Vec<Self::Event>, Error> {
        let mut events_to_process = Vec::new();
        let mut tx = self.conn.begin().await?;

        Self::extract_events(
            &mut tx,
            block,
            addresses,
            &self.trx_dispatch_contract,
            &mut events_to_process,
        )
        .await?;

        let mut sequence_id = self.current_sequence;
        for event in events_to_process.iter_mut() {
            assert!(
                !event.orphaned,
                "orphanned events should not be generated during processing"
            );
            event.project(&mut tx, self.provider.deref()).await?;
            event.sequence_id = sequence_id;
            sequence_id += 1;
        }

        db::create_events(&mut tx, &events_to_process).await?;
        tx.commit().await?;
        self.current_sequence = sequence_id;
        Ok(events_to_process)
    }

    async fn reprocess(
        &mut self,
        addresses: &HashMap<Address, Option<String>>,
        block: &BlockInfo,
    ) -> Result<Vec<Self::Event>, Error> {
        let mut events_to_process = Vec::new();
        let mut processed_events = HashSet::new();
        let mut tx = self.conn.0.begin().await?;

        // Remove old blocks and get corresponding TXs
        db::get_events_at_block(&mut tx, block.number())
            .await?
            .into_iter()
            .for_each(|e| {
                // Cancel Out Previous Action / OrphanedAction with the same TX Hash + Action
                // using the Event's Hash implementation
                if !processed_events.remove(&e) {
                    processed_events.insert(e);
                }
            });

        // Process txs in new blocks and unmark duplicate ones
        Self::extract_events(
            &mut tx,
            block,
            addresses,
            &self.trx_dispatch_contract,
            &mut events_to_process,
        )
        .await?;

        let mut events_to_process = Self::organize_events(events_to_process, processed_events);
        let mut sequence_id = self.current_sequence;
        for event in events_to_process.iter_mut() {
            assert!(
                !event.orphaned,
                "orphanned events should not be generated during reprocessing"
            );
            event.project(&mut tx, self.provider.deref()).await?;
            event.sequence_id = sequence_id;
            sequence_id += 1;
        }

        db::create_events(&mut tx, &events_to_process).await?;
        tx.commit().await?;
        self.current_sequence = sequence_id;
        Ok(events_to_process)
    }

    async fn process_fork(
        &mut self,
        addresses: &HashMap<Address, Option<String>>,
        mut blocks: Vec<BlockInfo>,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if blocks.is_empty() {
            log::warn!("empty block array given for fork processing");
            return Ok(Vec::new());
        }

        let mut events_to_process = Vec::new();
        let mut processed_events = HashSet::new();
        let mut tx = self.conn.0.begin().await?;
        let begin_block_num = blocks.last().expect("checked not empty").number();

        // Remove old blocks and get corresponding TXs
        db::remove_blocks_since(&mut tx, begin_block_num).await?;
        db::get_events_since_block(&mut tx, begin_block_num)
            .await?
            .into_iter()
            .for_each(|e| {
                // Cancel Out Previous Action / OrphanedAction with the same TX Hash + Action
                // using the Event's Hash implementation
                if !processed_events.remove(&e) {
                    processed_events.insert(e);
                }
            });

        // Process txs in new blocks and unmark duplicate ones
        blocks.reverse();
        for block in blocks {
            Self::extract_events(
                &mut tx,
                &block,
                addresses,
                &self.trx_dispatch_contract,
                &mut events_to_process,
            )
            .await?;
        }

        let mut events_to_process = Self::organize_events(events_to_process, processed_events);
        let mut sequence_id = self.current_sequence;
        for event in events_to_process.iter_mut() {
            if event.orphaned {
                event.unproject(&mut tx).await?;
            } else {
                event.project(&mut tx, self.provider.deref()).await?;
            }
            event.sequence_id = sequence_id;
            sequence_id += 1;
        }

        db::create_events(&mut tx, &events_to_process).await?;
        tx.commit().await?;
        self.current_sequence = sequence_id;
        Ok(events_to_process)
    }

    async fn get_newest_processed_block(&self) -> Result<Option<Self::Block>, Self::Error> {
        self.conn
            .get_newest_block()
            .await
            .map(|b| b.map(BlockInfo::new_dummy))
            .map_err(Into::into)
    }

    async fn log_activity<D: Serialize + Send + Sync + 'static>(
        &self,
        kind: &str,
        data: Option<D>,
    ) -> Result<(), Self::Error> {
        db::insert_activity_log(&self.conn.0, kind, Utc::now(), data)
            .await
            .map_err(Into::into)
    }

    fn storage_handle(&self) -> Self::StorageHandle {
        StorageHandle {
            conn: self.conn.clone(),
            provider: self.provider.clone(),
        }
    }
}

#[derive(Clone)]
pub struct StorageHandle {
    conn: PgPool,
    provider: Arc<Client>,
}

#[async_trait]
impl<T> StorageHandleT for StorageHandle<T>
where
    T: TRC20Meta + Send + Sync + 'static,
{
    type Error = Error;
    type Address = Address;

    async fn commit_addresses(
        &self,
        addresses: impl Iterator<Item = &'async_trait (Address, Option<String>)> + Send + 'async_trait,
    ) -> Result<(), Error> {
        let mut tx = self.conn.0.begin().await?;

        for addr in addresses {
            db::insert_address(&mut tx, &addr.0, addr.1.as_deref()).await?;
        }

        tx.commit().await.map_err(Into::into)
    }

    async fn commit_tokens(
        &self,
        addresses: impl Iterator<Item = &'async_trait Address> + Send + 'async_trait,
    ) -> Result<(), Error> {
        lazy_static::lazy_static! {
            static ref DEFAULT_METADATA: (String, String, u8) =
                ("NA".to_owned(), "NA".to_owned(), 0);
        }
        let mut tx = self.conn.0.begin().await?;

        //#FIXME perhaps change this to run in parallel
        for addr in addresses {
            let (name, symbol, decimal) = self
                .provider
                .get_trc20_metadata(*addr)
                .await
                .unwrap_or_else(|| DEFAULT_METADATA.clone());
            db::insert_trc20_token(&mut tx, addr, &name, &symbol, decimal).await?;
        }

        tx.commit().await.map_err(Into::into)
    }

    async fn latest_commited_sequence(&self) -> Result<i64, Error> {
        self.conn.get_latest_sequence_id().await.map_err(Into::into)
    }
}
#[cfg(test)]
mod test {
    use super::*;

    use std::str::FromStr;

    use atb::logging::init_logger;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::{Connection, PgPool};

    type Harvester = crate::harvester::Harvester<Processor<Provider>, Client>;

    async fn setup_harvester(pg_pool: sqlx::PgPool, start_block: Option<u64>) -> Harvester {
        // const NODE_URL: &'static str = "http://localhost:9090";
        const NODE_URL: &str = "https://api.shasta.trongrid.io";
        // let mut address_book = HashSet::<H160, _>::new();

        // address_book.insert(
        //     TronAddress::from_str("TXvdw1KJQpaKinFZ8hthUu6aG9kKshynDK")
        //         .unwrap()
        //         .into(),
        // );

        let client = Client::new(
            "Tron".to_owned(),
            NODE_URL,
            None,
            None,
            HashMap::new(),
            HashSet::default(),
            None,
            vec![],
        )
        .expect("should create client");
        let block_processor = Processor::<Provider>::new(pg_pool.into(), NODE_URL.into(), None)
            .await
            .expect("should create processor");

        Harvester::new(
            client,
            block_processor,
            start_block,
            0,
            3000,
            "TRON_TEST".to_owned(),
        )
        .await
        .expect("should create harvester")
    }

    async fn connect_postgres() -> (PgConnection, PgPool) {
        let database_url = "postgres://postgres:123456@localhost:5432/tron?sslmode=disable";
        let conn = PgConnection::connect(database_url)
            .await
            .expect("postgres available");

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .expect("postgres available");
        (conn, pool)
    }

    #[test]
    fn test_all() {}

    #[test]
    fn test_parse_log() {}
}
