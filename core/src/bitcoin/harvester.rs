use std::time::Duration;

use super::{
    client::{Client, Processor},
    types::BlockInfo,
};
use crate::Command;

use tokio::{sync::mpsc, time::sleep};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Sqlx error: `{0}`")]
    Sqlx(#[from] sqlx::error::Error),

    #[error("Block that should exist doesn't, according to RPC {0}")]
    MissingBlock(u64),

    #[error("Invalid desired height")]
    InvalidDesiredHeight,

    #[error("Client: `{0}`")]
    Client(super::client::Error),

    #[error("Other: `{0}`")]
    Other(#[from] Box<dyn std::error::Error + Sync + Send>),
}

pub struct Harvester {
    sender: mpsc::Sender<Command>,
    receiver: Option<mpsc::Receiver<Command>>,
    client: Client,
    block_processor: Processor,
    start_height: Option<u64>,
    end_height: Option<u64>,
    sleep_ms: u64,
    name: String,
    last_processed_block: Option<BlockInfo>,
    sweep_status: Option<u64>,
}

impl Harvester {
    pub fn new(
        sender: mpsc::Sender<Command>,
        receiver: Option<mpsc::Receiver<Command>>,
        client: Client,
        block_processor: Processor,
        start_height: Option<u64>,
        end_height: Option<u64>,
        sleep_ms: u64,
        name: String,
    ) -> Self {
        Self {
            sender,
            receiver,
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
                            log::warn!("🛑 Harvester {} Terminated", self.name);
                            return Ok(());
                        },
                        Some(Command::ScanBlock(from, to)) => {
                            self.start_height = Some(from);
                            self.end_height = Some(to);
                        },
                        Some(Command::Pause) => {
                            self.end_height = self.last_processed_block.as_ref().map(|b|b.number);
                        },
                        Some(Command::AutoScan) => {
                            self.end_height = None;
                            self.start_height = if let Some(b) = self.last_processed_block.as_ref(){
                                Some(b.number + 1)
                            }else {
                                Some(0)
                            };
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
            h
        } else {
            self.client
                .get_tip_number()
                .map_err(|e| Error::Client(e.into()))?
        };

        log::info!("harvester desired_height: {}", desired_height);

        //
        let (prev_block, process_first) = match self.last_processed_block {
            Some(ref mut block) => (block, false),
            None => {
                // first time ever handling events
                let start_num = self.start_height.unwrap_or(desired_height);
                let start_block = self
                    .client
                    .scan_block(Some(start_num))
                    .await
                    .map_err(|e| Error::Client(e.into()))?
                    .expect("desired_height is always <= tip_height. qed");

                (self.last_processed_block.insert(start_block), true)
            }
        };

        Self::process_blocks(
            &self.name,
            &mut self.client,
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
        client: &mut Client,
        block_processor: &mut BP,
        prev_block: &mut BlockInfo,
        desired_height: u64,
        process_first: bool,
    ) -> Result<(), Error> {
        let mut events = Vec::new();
        if process_first {
            log::trace!("🤖 {} Processing first block {}", name, prev_block.number);
            block_processor
                .process(prev_block)
                .await
                .map(|evts| events.extend(evts))
                .map_err(Into::into)?;
            log::trace!("✅ {} Finished Processing first block", name);
        }

        // Otherwise process until we reach the head
        while prev_block.number < desired_height {
            let block_num = prev_block.number + 1;

            let block = client
                .scan_block(Some(block_num))
                .await
                .map_err(|e| Error::Client(e.into()))?
                .ok_or_else(|| Error::MissingBlock(block_num))?;

            if prev_block.hash == block.parent_hash() {
                log::trace!("🤖 {} Processing block {}", name, block_num);
                block_processor
                    .process(&block)
                    .await
                    .map(|evts| events.extend(evts))
                    .map_err(Into::into)?;
                log::trace!("✅ {} Finished Processing block", name);
            } else {
                log::info!("❗️{} Detected fork - Preparing reorg", name);
                // Self::handle_reorg(name, client, block_processor, block.clone()).await
            }

            //Just store the whole Block type, convert the interface to return &Hash
            //instead of Hash (copied)
            *prev_block = block
        }
        Ok(events)
    }
}
