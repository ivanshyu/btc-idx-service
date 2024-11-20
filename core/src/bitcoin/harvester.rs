use super::client::{Client, Processor};
use crate::Command;

use tokio::sync::mpsc;

pub struct Harvester {
    sender: mpsc::Sender<Command>,
    receiver: Option<mpsc::Receiver<Command>>,
    client: Client,
    block_processor: Processor,
    start_height: Option<u64>,
    sleep_ms: u64,
    name: String,
    last_processed_block: Option<u64>,
    sweep_status: Option<u64>,
}
