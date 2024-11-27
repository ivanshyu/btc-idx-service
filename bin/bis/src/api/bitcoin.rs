use std::sync::Arc;

use actix_http::{Response, StatusCode};
use actix_web::{
    error, get, post,
    web::{self, Data},
    Error, HttpRequest, HttpResponse, Responder,
};
use serde::Deserialize;
use sqlx::PgPool;

pub fn routes(scope: &'static str, pg_pool: PgPool) -> impl FnOnce(&mut web::ServiceConfig) {
    move |config: &mut web::ServiceConfig| {
        let scope = web::scope(scope).app_data(Data::new(pg_pool));

        config.service(scope);
    }
}
