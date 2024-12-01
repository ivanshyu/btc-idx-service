use super::types::{AggregatorMsg, BtcP2trEvent};
use crate::sqlx_postgres::bitcoin as db;

use std::sync::Arc;

use atb_tokio_ext::{Shutdown, ShutdownComplete};
use atb_types::{prelude::chrono::Timelike, Utc};
use futures::FutureExt;
use futures_core::future::BoxFuture;
use sqlx::PgPool;
use tokio::sync::{mpsc, Notify};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Sqlx error: `{0}`")]
    Sqlx(#[from] sqlx::error::Error),

    #[error("Other: `{0}`")]
    Other(#[from] Box<dyn std::error::Error + Sync + Send>),
}

pub struct Aggregator {
    receiver: mpsc::UnboundedReceiver<AggregatorMsg>,
    pool: PgPool,
    shutdown_notify: Arc<Notify>,
    last_block_number: usize,
}

impl Aggregator {
    pub fn new(
        pool: PgPool,
        receiver: mpsc::UnboundedReceiver<AggregatorMsg>,
        shutdown_notify: Arc<Notify>,
    ) -> Self {
        Self {
            receiver,
            pool,
            shutdown_notify,
            last_block_number: 0,
        }
    }

    pub async fn start(mut self) -> Result<(), Error> {
        loop {
            tokio::select! {
                biased;

                event = self.receiver.recv() => {
                    match event {
                        Some(AggregatorMsg::Event(event)) => self.handle_event(event).await?,
                        Some(AggregatorMsg::Reorg(block_number)) => {
                            self.handle_reorg(block_number).await?
                        }
                        None => break,
                    }
                }

                _ = self.shutdown_notify.notified() => {
                    log::info!("Shutting down aggregator gracefully.");
                    break;
                }
            }
        }
        self.cleanup().await;
        Ok(())
    }

    async fn handle_event(&mut self, event: BtcP2trEvent) -> Result<(), Error> {
        let mut tx = self.pool.begin().await?;

        let block_number = event.block_number;

        if event.is_coinbase {
            self.insert_pending_event(&mut tx, event.clone()).await?;
        } else {
            self.insert_event(&mut tx, event.clone()).await?;
            self.update_total_balance(&mut tx, event.clone()).await?;
            self.update_statistic_balance_change(&mut tx, event).await?;
        }
        tx.commit().await?;

        if block_number > self.last_block_number {
            self.last_block_number = block_number;
            self.commit_pending_events().await?;
        }

        Ok(())
    }

    async fn handle_reorg(&mut self, block_number: usize) -> Result<(), Error> {
        let mut tx = self.pool.begin().await?;

        let mut events = db::get_p2tr_events_since_block(&mut *tx, block_number).await?;

        let last_block_number = events.last().unwrap().block_number;
        events.iter_mut().for_each(|event| event.reorg());

        for event in events {
            self.insert_event(&mut tx, event.clone()).await?;

            self.update_total_balance(&mut tx, event.clone()).await?;
            self.update_statistic_balance_change(&mut tx, event).await?;
        }
        let _ = db::pull_pending_p2tr_events(&mut *tx, last_block_number).await?;

        tx.commit().await?;

        self.last_block_number = last_block_number;

        Ok(())
    }

    // for coinbase event
    async fn insert_pending_event(
        &mut self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        event: BtcP2trEvent,
    ) -> Result<(), Error> {
        log::debug!("Inserted pending event with id: {}", &event.txid);

        db::create_pending_p2tr_event(tx.as_mut(), event).await?;
        Ok(())
    }

    // for non-coinbase event
    async fn insert_event(
        &mut self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        event: BtcP2trEvent,
    ) -> Result<(), Error> {
        let event_id = db::create_p2tr_event(tx.as_mut(), event).await?;
        log::debug!("Inserted event with id: {}", event_id);
        Ok(())
    }

    async fn update_total_balance(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        event: BtcP2trEvent,
    ) -> Result<(), Error> {
        // send: negative, receive: positive
        let balance = event.amount;
        db::increment_btc_balance(tx.as_mut(), &event.address, &balance, Utc::now()).await?;
        Ok(())
    }

    async fn update_statistic_balance_change(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        event: BtcP2trEvent,
    ) -> Result<(), Error> {
        // send: negative, receive: positive
        let balance = event.amount;

        // #NOTE: unwrap is safe here
        let now = Utc::now();
        let current_hour = now
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap();

        db::increment_static_balances(tx.as_mut(), &event.address, &balance, current_hour, now)
            .await?;
        Ok(())
    }

    async fn commit_pending_events(&mut self) -> Result<(), Error> {
        if self.last_block_number < 100 {
            return Ok(());
        }

        let events = db::pull_pending_p2tr_events(&self.pool, self.last_block_number - 100).await?;

        if events.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for event in events {
            log::info!("Committing pending event with id: {}", &event.txid);
            self.insert_event(&mut tx, event.clone()).await?;
            self.update_total_balance(&mut tx, event.clone()).await?;
            self.update_statistic_balance_change(&mut tx, event).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn cleanup(mut self) {
        log::info!("Cleaning up resources...");
        while let Some(event) = self.receiver.recv().await {
            // TODO: handle error
            match event {
                AggregatorMsg::Event(event) => {
                    let _ = self.handle_event(event).await;
                }
                AggregatorMsg::Reorg(block_number) => {
                    let _ = self.handle_reorg(block_number).await;
                }
            }
        }
        log::info!("Cleaning up resources... done");
    }

    pub fn to_boxed_task_fn(
        self,
    ) -> impl FnOnce(Shutdown, ShutdownComplete) -> BoxFuture<'static, ()> {
        move |shutdown, shutdown_complete| self.run(shutdown, shutdown_complete).boxed()
    }

    pub async fn run(self, mut shutdown: Shutdown, _shutdown_complete: ShutdownComplete) {
        tokio::select! {
            res = self.start() => {
                match res {
                    Ok(s) => s,
                    Err(e)=>{
                        // probably fatal error occurs
                        panic!("aggregator stopped: {:?}", e)
                    }
                };
            },

            _ = shutdown.recv() => {
                log::warn!("aggregator shutting down from signal");
            }
        }
        log::info!("aggregator stopped")
    }
}
