pub mod client;

use crate::{
    bitcoin::harvester::client::{Client, Processor},
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

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Sqlx error: `{0}`")]
    Sqlx(#[from] sqlx::error::Error),

    #[error("Block that should exist doesn't, according to RPC {0}")]
    MissingBlock(usize),

    #[error("Invalid desired height")]
    InvalidDesiredHeight,

    #[error("Client: `{0}`")]
    Client(#[from] client::Error),

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
    sweep_status: Option<u64>,
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
            sweep_status: None,
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
            self.client
                .get_tip_number()
                .await
                .map_err(|e| Error::Client(e.into()))?
        };

        log::info!("harvester desired height: {}", desired_height);
        //
        let (prev_block, process_first) = match self.last_processed_block {
            Some(ref mut block) => (block, false),
            None => {
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
                let start_block = self
                    .client
                    .scan_block(Some(start_num))
                    .await
                    .map_err(|e| Error::Client(e.into()))?;

                (self.last_processed_block.insert(start_block), true)
            }
        };

        Self::process_blocks(
            &self.name,
            &self.client,
            &mut self.block_processor,
            prev_block,
            desired_height,
            process_first,
        )
        .await?;

        Ok(())
    }

    pub async fn process_blocks(
        name: &str,
        client: &Client,
        block_processor: &mut Processor,
        prev_block: &mut BlockInfo,
        desired_height: usize,
        process_first: bool,
    ) -> Result<(), Error> {
        if process_first {
            log::info!(
                "🤖 {} Processing first block {}",
                name,
                prev_block.header.height
            );
            block_processor.process(prev_block).await?;
            log::info!("✅ {} Finished Processing first block", name);
        }

        // Otherwise process until we reach the head
        while prev_block.header.height < desired_height {
            let block_num = prev_block.header.height + 1;

            let block = client
                .scan_block(Some(block_num))
                .await
                .map_err(|e| Error::Client(e.into()))?;

            if let Some(prev) = block.header.previous_block_hash {
                if prev_block.header.hash == prev {
                    log::info!("🤖 {} Processing block {}", name, block_num);
                    block_processor.process(&block).await?;
                    log::info!("✅ {} Finished Processing block", name);
                } else {
                    log::info!("❗️{} Detected fork - Preparing reorg", name);
                    // Self::handle_reorg(name, client, block_processor, block.clone()).await
                }
            }

            //Just store the whole Block type, convert the interface to return &Hash
            //instead of Hash (copied)
            *prev_block = block
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

        let start_block = self
            .client
            .scan_block(Some(from))
            .await
            .map_err(|e| Error::Client(e.into()))?;

        let _ = self.last_processed_block.insert(start_block);
        Ok(())
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
