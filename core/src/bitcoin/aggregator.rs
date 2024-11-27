use super::types::{Action, BtcP2trEvent};
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
    receiver: mpsc::UnboundedReceiver<BtcP2trEvent>,
    pool: PgPool,
    shutdown_notify: Arc<Notify>,
}

impl Aggregator {
    pub fn new(
        pool: PgPool,
        receiver: mpsc::UnboundedReceiver<BtcP2trEvent>,
        shutdown_notify: Arc<Notify>,
    ) -> Self {
        Self {
            receiver,
            pool,
            shutdown_notify,
        }
    }

    pub async fn start(mut self) -> Result<(), Error> {
        loop {
            tokio::select! {
                biased;

                event = self.receiver.recv() => {
                    match event {
                        Some(event) => self.handle_event(event).await?,
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
        self.insert_event(&mut tx, event.clone()).await?;
        self.update_total_balance(&mut tx, event.clone()).await?;
        self.update_statistic_balance_change(&mut tx, event).await?;
        tx.commit().await?;
        Ok(())
    }

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
        let balance = match event.action {
            Action::Send => -event.amount,
            Action::Receive => event.amount,
        };
        db::increment_btc_balance(tx.as_mut(), &event.address, &balance, Utc::now()).await?;
        Ok(())
    }

    async fn update_statistic_balance_change(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        event: BtcP2trEvent,
    ) -> Result<(), Error> {
        let balance = match event.action {
            Action::Send => -event.amount,
            Action::Receive => event.amount,
        };

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

    async fn cleanup(mut self) {
        log::info!("Cleaning up resources...");
        while let Some(event) = self.receiver.recv().await {
            // TODO: handle error
            let _ = self.handle_event(event).await;
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
