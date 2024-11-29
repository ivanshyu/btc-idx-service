pub mod bitcoin;
pub mod rpc_client;
pub mod sqlx_postgres;

use std::fmt::Debug;

use async_trait::async_trait;
use sqlx::PgPool;
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum Command {
    Terminate,
    Pause,
    // from , to
    ScanBlock(usize, usize),
    AutoScan,
}

#[derive(Clone, Debug)]
// handle `indexer scan-block —-from 123456 -—to 124456` from cli
pub struct CommandHandler {
    sender: mpsc::Sender<Command>,
    pub storage: HandlerStorage,
}

impl CommandHandler {
    pub fn new(sender: mpsc::Sender<Command>, storage: HandlerStorage) -> Self {
        Self { sender, storage }
    }

    pub async fn terminate(&self) -> anyhow::Result<()> {
        self.sender
            .send(Command::Terminate)
            .await
            .map_err(Into::into)
    }

    pub async fn pause(&self) -> anyhow::Result<()> {
        self.sender.send(Command::Pause).await.map_err(Into::into)
    }

    pub async fn scan_block(&self, from: usize, to: usize) -> anyhow::Result<()> {
        self.sender
            .send(Command::ScanBlock(from, to))
            .await
            .map_err(Into::into)
    }

    pub async fn auto_scan(&self) -> anyhow::Result<()> {
        self.sender
            .send(Command::AutoScan)
            .await
            .map_err(Into::into)
    }
}

#[derive(Debug, Clone)]
// for db interaction
pub struct HandlerStorage {
    pub conn: PgPool,
}

impl HandlerStorage {
    pub async fn latest_commited_sequence(&self) -> Result<i64, anyhow::Error> {
        todo!()
    }

    pub async fn latest_commited_block(&self) -> Result<u64, anyhow::Error> {
        todo!()
    }
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
