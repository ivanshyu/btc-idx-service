use crate::api::{
    error::ApiError,
    models::{BlockHeader, LatestBlockHeight, Transaction},
};
use bis_core::{sqlx_postgres::bitcoin as db, CommandHandler};

use actix_web::{
    get, post,
    web::{self, Data, Path},
    HttpResponse, Responder,
};
use bitcoin::{BlockHash, Txid};
use serde_json::json;

pub fn routes(
    scope: &'static str,
    handler: CommandHandler,
) -> impl FnOnce(&mut web::ServiceConfig) {
    move |config: &mut web::ServiceConfig| {
        let scope = web::scope(scope)
            .app_data(Data::new(handler))
            .service(raw::latest_block)
            .service(raw::block)
            .service(raw::transaction)
            .service(processed::p2tr_balance)
            .service(processed::p2tr_utxo)
            .service(aggregated::balance_snapshots)
            .service(protected::pause_harvester)
            .service(protected::resume_harvester)
            .service(protected::scan_block)
            .service(protected::terminate_harvester);

        config.service(scope);
    }
}

pub mod raw {
    use super::*;
    #[get("raw/block/latest")]
    pub async fn latest_block(handler: Data<CommandHandler>) -> Result<impl Responder, ApiError> {
        db::get_latest_block_num(&handler.storage.conn)
            .await
            .map(|n| {
                HttpResponse::Ok().json(LatestBlockHeight {
                    latest_block_height: n,
                })
            })
            .map_err(Into::into)
    }

    #[get("raw/block/{block_hash}")]
    pub async fn block(
        block_hash: Path<BlockHash>,
        handler: Data<CommandHandler>,
    ) -> Result<impl Responder, ApiError> {
        db::get_block(&handler.storage.conn, &block_hash)
            .await
            .map(|b| {
                HttpResponse::Ok().json(BlockHeader {
                    block_header_data: b.into(),
                })
            })
            .map_err(Into::into)
    }

    #[get("raw/transaction/{tx_id}")]
    pub async fn transaction(
        tx_id: Path<Txid>,
        handler: Data<CommandHandler>,
    ) -> Result<impl Responder, ApiError> {
        db::get_transaction(&handler.storage.conn, &tx_id)
            .await
            .map(|tx| {
                HttpResponse::Ok().json(Transaction {
                    transaction_data: tx.into(),
                })
            })
            .map_err(Into::into)
    }
}

pub mod processed {

    use crate::api::models::{CurrentBalance, Utxo};
    use bis_core::bitcoin::types::BTC_NETWORK;

    use std::str::FromStr;

    use bitcoin::Address;

    use super::*;

    #[get("processed/p2tr/{address}/balance")]
    pub async fn p2tr_balance(
        address: Path<String>,
        handler: Data<CommandHandler>,
    ) -> Result<impl Responder, ApiError> {
        let address = Address::from_str(&address)
            .and_then(|a| a.require_network(*BTC_NETWORK.get().unwrap()))
            .map_err(|e| ApiError::ParamsInvalid(e.into()))?;

        db::get_btc_balance(&handler.storage.conn, &address.to_string())
            .await?
            .map(|b| HttpResponse::Ok().json(CurrentBalance::from(b)))
            .ok_or_else(|| ApiError::NotFound("Balance not found".into()))
    }

    #[get("processed/p2tr/{address}/utxo")]
    pub async fn p2tr_utxo(
        address: Path<String>,
        handler: Data<CommandHandler>,
    ) -> Result<impl Responder, ApiError> {
        let address = Address::from_str(&address)
            .and_then(|a| a.require_network(*BTC_NETWORK.get().unwrap()))
            .map_err(|e| ApiError::ParamsInvalid(e.into()))?;

        db::get_unspent_utxos_by_owner(&handler.storage.conn, &address.to_string())
            .await
            .map(|u| u.into_iter().map(Utxo::from).collect::<Vec<_>>())
            .map(|u| HttpResponse::Ok().json(u))
            .map_err(Into::into)
    }
}

pub mod aggregated {
    use super::*;
    use crate::api::models::{AggregatedBalance, AggregatedBalanceParams, Granularity, TimeSpan};
    use atb_cli::DateTime;
    use bis_core::bitcoin::types::BTC_NETWORK;
    use sqlx::types::chrono;

    use std::str::FromStr;

    use bitcoin::Address;
    use web::Query;

    #[get("aggregated/p2tr/{address}")]
    pub async fn balance_snapshots(
        address: Path<String>,
        query: Query<AggregatedBalanceParams>,
        handler: Data<CommandHandler>,
    ) -> Result<impl Responder, ApiError> {
        let AggregatedBalanceParams {
            time_span,
            granularity,
        } = query.into_inner();

        let (interval, step, step_interval) = match (time_span, granularity) {
            (TimeSpan::M, Granularity::W) => ("1 month", "week", "1 week"),
            (TimeSpan::M, Granularity::D) => ("1 month", "day", "1 day"),
            (TimeSpan::M, Granularity::H) => ("1 month", "hour", "1 hour"),
            (TimeSpan::W, Granularity::W) => ("1 week", "week", "1 week"),
            (TimeSpan::W, Granularity::D) => ("1 week", "day", "1 day"),
            (TimeSpan::W, Granularity::H) => ("1 week", "hour", "1 hour"),
            (TimeSpan::D, Granularity::D) => ("1 day", "day", "1 day"),
            (TimeSpan::D, Granularity::H) => ("1 day", "hour", "1 hour"),
            _ => {
                return Err(ApiError::ParamsInvalid(
                    "Invalid time span or granularity combination".into(),
                ));
            }
        };

        let address = Address::from_str(&address)
            .and_then(|a| a.require_network(*BTC_NETWORK.get().unwrap()))
            .map_err(|e| ApiError::ParamsInvalid(e.into()))?;

        let total = db::get_btc_balance(&handler.storage.conn, &address.to_string())
            .await?
            .map(|b| b.amount)
            .unwrap_or_default();

        let mut snapshots = serde_json::Map::new();
        let mut running_balance = total;
        db::get_balance_snapshots(
            &handler.storage.conn,
            &address.to_string(),
            interval,
            step,
            step_interval,
        )
        .await?
        .into_iter()
        .for_each(|(datetime, change)| {
            let current_balance = running_balance.clone();
            running_balance -= &change;
            snapshots.insert(
                datetime.timestamp().to_string(),
                // taipei_time.to_string(),
                serde_json::to_value(vec![AggregatedBalance {
                    balance: current_balance,
                }])
                .unwrap(),
            );
        });

        Ok(HttpResponse::Ok().json(snapshots))
    }
}

pub mod protected {
    use super::*;

    #[post("protected/harvester/pause")]
    pub async fn pause_harvester(
        handler: Data<CommandHandler>,
    ) -> Result<impl Responder, ApiError> {
        handler
            .pause()
            .await
            .map_err(|e| ApiError::Other(e.into()))?;
        Ok(HttpResponse::Ok().body("OK"))
    }

    #[post("protected/harvester/resume")]
    pub async fn resume_harvester(
        handler: Data<CommandHandler>,
    ) -> Result<impl Responder, ApiError> {
        handler
            .auto_scan()
            .await
            .map_err(|e| ApiError::Other(e.into()))?;
        Ok(HttpResponse::Ok().body("OK"))
    }

    #[post("protected/harvester/scan/{from}/{to}")]
    pub async fn scan_block(
        from: Path<(usize, usize)>,
        handler: Data<CommandHandler>,
    ) -> Result<impl Responder, ApiError> {
        let (from, to) = *from;
        if from > to {
            return Err(ApiError::ParamsInvalid(
                "From block number must be less than to block number".into(),
            ));
        }

        handler
            .scan_block(from, to)
            .await
            .map_err(|e| ApiError::Other(e.into()))?;
        Ok(HttpResponse::Ok().body("OK"))
    }

    #[post("protected/harvester/terminate")]
    pub async fn terminate_harvester(
        handler: Data<CommandHandler>,
    ) -> Result<impl Responder, ApiError> {
        handler
            .terminate()
            .await
            .map_err(|e| ApiError::Other(e.into()))?;
        Ok(HttpResponse::Ok().body("OK"))
    }
}
