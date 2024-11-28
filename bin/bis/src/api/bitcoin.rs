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
            .service(latest_block)
            .service(block)
            .service(transaction);

        config.service(scope);
    }
}

// raw
#[get("raw/block/latest")]
async fn latest_block(handler: Data<CommandHandler>) -> Result<impl Responder, ApiError> {
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
async fn block(
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
async fn transaction(
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
