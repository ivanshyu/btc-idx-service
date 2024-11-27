pub mod bitcoin;
pub mod models;

use actix_cors::Cors;
use actix_http::header;
use actix_web::{
    http::header::ContentType,
    web::{self},
    App, HttpResponse, HttpServer,
};
use bis_core::CommandHandler;
use sqlx::postgres::PgPool;

#[derive(Clone)]
pub struct ServiceConfig {
    pub handler: CommandHandler,
}

impl ServiceConfig {
    fn service(&self) -> actix_web::Scope {
        web::scope("api/v1").configure(bitcoin::routes("bitcoin", self.handler.clone()))
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
                    .allow_any_origin()
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
