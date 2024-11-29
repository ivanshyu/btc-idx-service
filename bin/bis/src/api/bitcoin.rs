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
            .service(processed::p2tr_utxo);

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

    use bitcoin::{address::NetworkChecked, Address, ScriptBuf};

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
