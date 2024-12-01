pub mod client;
pub mod processor;

use crate::{
    bitcoin::harvester::{client::Client, processor::Processor},
    bitcoin::types::BlockInfo,
    sqlx_postgres::bitcoin as db,
};
use crate::{Command, CommandHandler};

use std::{cmp::max, sync::Arc, time::Duration};

use atb_tokio_ext::{Shutdown, ShutdownComplete};
use futures::FutureExt;
use futures_core::future::BoxFuture;
use tokio::{
    sync::mpsc::{self},
    time::sleep,
};

use super::types::BtcP2trEvent;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Sqlx error: `{0}`")]
    Sqlx(#[from] sqlx::error::Error),

    #[error("Block that should exist doesn't, according to RPC {0}")]
    MissingBlock(usize),

    #[error("Invalid desired height")]
    InvalidDesiredHeight,

    #[error("Client: `{0}`")]
    Client(#[from] processor::Error),

    #[error("Other: `{0}`")]
    Other(#[from] Box<dyn std::error::Error + Sync + Send>),
}

pub struct Harvester {
    sender: mpsc::Sender<Command>,
    receiver: Option<mpsc::Receiver<Command>>,
    client: Arc<Client>,
    block_processor: Processor,
    start_height: Option<usize>,
    end_height: Option<usize>,
    sleep_ms: u64,
    name: String,
    last_processed_block: Option<BlockInfo>,
}

impl Harvester {
    pub async fn new(
        client: Arc<Client>,
        block_processor: Processor,
        start_height: Option<usize>,
        end_height: Option<usize>,
        sleep_ms: u64,
        name: String,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(100);

        Self {
            sender,
            receiver: Some(receiver),
            client,
            block_processor,
            start_height,
            end_height,
            sleep_ms,
            name,
            last_processed_block: None,
        }
    }

    pub async fn start(mut self) -> Result<(), Error> {
        let mut receiver = self.receiver.take().unwrap();

        log::info!("{} Harvester started", self.name);
        loop {
            tokio::select! {
                biased;

                cmd = receiver.recv() => {
                    match cmd {
                        // if end_height is None => keep scanning
                        Some(Command::Terminate) | None => {
                            log::info!("🤖 harvester {} terminated from COMMAND", self.name);
                            log::warn!("🛑 Harvester {} Terminated", self.name);
                            return Ok(());
                        },
                        Some(Command::ScanBlock(from, to)) => {
                            log::info!(
                                "🤖 harvester {} scanning from {} to {} from COMMAND",
                                self.name,
                                from,
                                to
                            );
                            self.scan_from(from).await?;
                            self.end_height = Some(to);
                        },
                        Some(Command::Pause) => {
                            log::info!("🤖 harvester {} paused from COMMAND", self.name);
                            self.end_height = self.last_processed_block.as_ref().map(|b|b.header.height);
                        },
                        Some(Command::AutoScan) => {
                            log::info!("🤖 harvester {} resumed from COMMAND", self.name);
                            self.end_height = None;
                        },
                    }
                }

                res = async {
                    sleep(Duration::from_millis(self.sleep_ms)).await;
                    self.harvest().await
                } => {
                    match &res {
                        Ok(_) => (),
                        Err(e) => {
                            log::error!("{} error: {}", self.name, e);
                            match e {
                                //safe to retry
                                Error::InvalidDesiredHeight
                                | Error::MissingBlock(_)
                                | Error::Client(_) => (),
                                //probably fatal
                                _ => return res.map_err(Into::into)
                            }
                        },
                    }
                }
            }
        }
    }

    pub async fn harvest(&mut self) -> Result<(), Error> {
        let desired_height = if let Some(h) = self.end_height {
            log::info!("harvester desired end height: {}", h);
            h
        } else {
            self.client.get_tip_number().await?
        };

        log::info!("harvester desired height: {}", desired_height);

        let process_first = if self.last_processed_block.is_none() {
            // first time ever handling events
            log::info!("🤖 harvester first time handling events after start");

            let last_processed_block =
                db::get_latest_block_num(&self.block_processor.db_connection().conn).await?;
            log::info!(
                "harvester last processed block in db: {}",
                last_processed_block.unwrap_or(0)
            );

            log::info!("harvester start_height in config: {:?}", self.start_height);

            let start_num = match last_processed_block {
                Some(num) => max(num + 1, self.start_height.unwrap_or(desired_height)),
                None => self.start_height.unwrap_or(desired_height),
            };

            log::info!("harvester start_num: {}", start_num);
            let start_block = self.client.scan_block(Some(start_num)).await?;

            self.last_processed_block = Some(start_block);
            true
        } else {
            false
        };

        if process_first {
            let block = self.last_processed_block.as_ref().unwrap();
            log::info!(
                "🤖 {} Processing first block {}",
                self.name,
                block.header.height
            );
            self.block_processor.process(block).await?;
            log::info!("✅ {} Finished Processing first block", self.name);
        }

        // 處理後續區塊
        while let Some(prev_block) = &self.last_processed_block {
            if prev_block.header.height >= desired_height {
                break;
            }

            let block_num = prev_block.header.height + 1;
            let block = self.client.scan_block(Some(block_num)).await?;

            if let Some(prev) = block.header.previous_block_hash {
                if prev_block.header.hash == prev {
                    log::info!("🤖 {} Processing block {}", self.name, block_num);
                    self.block_processor.process(&block).await?;
                    log::info!("✅ {} Finished Processing block", self.name);
                } else {
                    log::info!("❗️{} Detected fork - Preparing reorg", self.name);
                    // self.handle_reorg(block.clone()).await?;
                }
            }
            self.last_processed_block = Some(block);
        }

        Ok(())
    }

    pub async fn scan_from(&mut self, from: usize) -> Result<(), Error> {
        if let Some(last_processed_block) = &self.last_processed_block {
            if last_processed_block.header.height < from {
                log::warn!(
                    "🤖 harvester Scanning from orignal `{}` to specific block `{}`, it may have some gaps between them",
                    last_processed_block.header.height,
                    from
                );
                self.start_height = Some(from);
            }
        }

        let start_block = self.client.scan_block(Some(from)).await?;

        let _ = self.last_processed_block.insert(start_block);
        Ok(())
    }

    /// Handle blockchain reorganization when detected
    pub async fn handle_reorg(
        &mut self,
        head_block: BlockInfo,
    ) -> Result<Vec<BtcP2trEvent>, Error> {
        // If head has the same height but different hash, then there's a fork
        log::info!("❗️{} Reorg started", self.name);

        let head_block_number = head_block.header.height;
        let fork_blocks_reversed = self.get_fork_blocks(head_block).await?;

        let fork_origin = fork_blocks_reversed
            .last()
            .expect("blocks should exist. qed")
            .header
            .height;

        log::info!(
            "🔧 {} Reorg from {} to {}",
            self.name,
            fork_origin,
            head_block_number
        );
        // let events = self
        //     .block_processor
        //     .process_fork(fork_blocks_reversed)
        //     .await
        //     .map_err(|e| Error::Other(e.into()))?;
        let events = vec![];

        log::info!("💫 {} Reorg complete", self.name);
        Ok(events)
    }

    pub async fn get_fork_blocks(&self, block: BlockInfo) -> Result<Vec<BlockInfo>, Error> {
        let mut fork_blocks_reversed = Vec::new();
        let mut current_block = block;

        'found_origin: loop {
            loop {
                // Found common ancestor
                if current_block.header.height == 0
                    || self
                        .block_processor
                        .is_processed(&current_block.header.hash)
                        .await
                        .map_err(|e| Error::Other(e.into()))?
                {
                    log::trace!(
                        "🌟 {} Found common ancestor at block {}",
                        self.name,
                        current_block.header.height
                    );
                    break 'found_origin;
                }

                fork_blocks_reversed.push(current_block.clone());

                if let Some(parent_hash) = &current_block.header.previous_block_hash {
                    let parent_block =
                        self.client
                            .scan_block_by_hash(parent_hash)
                            .await
                            .map_err(|e| {
                                log::error!("failed to mine_block_by_hash {e}");
                                Error::Client(e)
                            })?;
                    current_block = parent_block;
                } else {
                    // unreacheable
                    break;
                }
            }

            // Another fork detected during rollback, restarting
            fork_blocks_reversed.clear();
            current_block = self.client.scan_block(None).await.map_err(Error::Client)?;

            log::trace!(
                "📦 {} Current tip: {}, hash: {}",
                self.name,
                current_block.header.height,
                current_block.header.hash
            );
        }

        Ok(fork_blocks_reversed)
    }

    // lifetime

    pub fn handle(&self) -> CommandHandler {
        CommandHandler {
            sender: self.sender.clone(),
            storage: self.block_processor.db_connection(),
        }
    }

    pub fn to_boxed_task_fn(
        self,
    ) -> impl FnOnce(Shutdown, ShutdownComplete) -> BoxFuture<'static, ()> {
        move |shutdown, shutdown_complete| self.run(shutdown, shutdown_complete).boxed()
    }

    pub async fn run(self, mut shutdown: Shutdown, _shutdown_complete: ShutdownComplete) {
        let name = self.name.clone();
        let handle = self.handle();
        tokio::select! {
            res = self.start() => {
                match res {
                    Ok(s) => s,
                    Err(e)=>{
                        // probably fatal error occurs
                        panic!("{} processor stopped: {:?}", name, e)
                    }
                };
            },

            _ = shutdown.recv() => {
                log::warn!("{} shutting down from signal", name);
                let _ = handle.terminate().await;
            }
        }
        log::info!("{} stopped", name)
    }
}
