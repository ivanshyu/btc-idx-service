pub mod bitcoin;
pub mod sqlx_postgres;

use std::fmt::Debug;
use std::hash::Hash;

use async_trait::async_trait;
use tokio::sync::mpsc;

#[derive(Debug)]
enum Command {
    Terminate,
    Pause,
    // from , to
    ScanBlock(u64, u64),
    AutoScan,
}

// handle `indexer scan-block —-from 123456 -—to 124456` from cli
pub struct CommandHandler<S: HandlerStorage + Clone> {
    sender: mpsc::Sender<Command>,
    storage: S,
}

// for db interaction
#[async_trait]
pub trait HandlerStorage {
    type Error: std::error::Error;

    type Address: Hash + Eq + Debug + Send + Sync + 'static;

    async fn commit_addresses(
        &self,
        addresses: impl Iterator<Item = &'async_trait (Self::Address, Option<String>)>
            + Send
            + 'async_trait,
    ) -> Result<(), Self::Error>;

    async fn commit_tokens(
        &self,
        addresses: impl Iterator<Item = &'async_trait Self::Address> + Send + 'async_trait,
    ) -> Result<(), Self::Error>;

    async fn latest_commited_sequence(&self) -> Result<i64, Self::Error>;

    async fn latest_commited_block(&self) -> Result<u64, Self::Error>;
}

// for calculate user balance
#[async_trait]
pub trait Projection {
    type Error: std::error::Error;
    type Store: Send + Sync + 'static;

    async fn project(&self, _conn: &mut Self::Store, _block_num: u64) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn unproject(&self, _conn: &mut Self::Store) -> Result<(), Self::Error> {
        Ok(())
    }
}
