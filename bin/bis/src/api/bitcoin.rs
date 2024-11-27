use bis_core::{sqlx_postgres::bitcoin as db, CommandHandler};

use std::sync::Arc;

use actix_http::{Response, StatusCode};
use actix_web::{
    error, get, post,
    web::{self, Data},
    Error, HttpRequest, HttpResponse, Responder,
};
use serde::Deserialize;
use sqlx::PgPool;

pub fn routes(
    scope: &'static str,
    handler: CommandHandler,
) -> impl FnOnce(&mut web::ServiceConfig) {
    move |config: &mut web::ServiceConfig| {
        let scope = web::scope(scope)
            .app_data(Data::new(handler))
            .service(latest_block);

        config.service(scope);
    }
}

#[get("raw/block/latest")]
async fn latest_block(handler: Data<CommandHandler>) -> Result<impl Responder, Error> {
    db::get_latest_block_num(&handler.storage.conn)
        .await
        .map(web::Json)
        .map_err(error::ErrorInternalServerError)
}
