pub mod bitcoin;

use std::sync::Arc;

use actix_cors::Cors;
use actix_http::header;
use actix_web::{
    http::header::ContentType,
    web::{self},
    App, HttpResponse, HttpServer,
};
use sqlx::postgres::PgPool;

#[derive(Clone)]
pub struct ServiceConfig {
    pub pg_pool: PgPool,
}

impl ServiceConfig {
    fn service(&self) -> actix_web::Scope {
        web::scope("api").configure(bitcoin::routes("bitcoin", self.pg_pool.clone()))
    }
}

pub async fn build_http_service(host: &str, service_config: ServiceConfig) -> std::io::Result<()> {
    let process_info: &'static str = Box::leak(
        serde_json::to_string_pretty(atb_cli::process_info())
            .expect("serialize ok. qed")
            .into_boxed_str(),
    );

    HttpServer::new(move || {
        App::new()
            .wrap(
                Cors::default()
                    .allowed_origin("http://127.0.0.1:3030")
                    .allowed_origin("http://localhost:3030")
                    .allowed_origin("http://localhost:3040")
                    .allowed_methods(vec!["POST", "GET"])
                    .allowed_header(header::CONTENT_TYPE)
                    .max_age(3600),
            )
            .route("/healthz", web::get().to(HttpResponse::Ok))
            .route(
                "/infoz",
                web::get().to(move || async move {
                    HttpResponse::Ok()
                        .insert_header(ContentType::json())
                        .body(process_info)
                }),
            )
            .service(service_config.service())
    })
    .bind(host)?
    .run()
    .await
}
