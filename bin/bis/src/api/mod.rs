pub mod bitcoin;
pub mod error;
pub mod models;

use actix_cors::Cors;
use actix_http::header;
use actix_web::{
    http::header::ContentType,
    middleware::Logger,
    web::{self},
    App, HttpResponse, HttpServer,
};
use bis_core::CommandHandler;
use error::ApiError;

#[derive(Clone)]
pub struct ServiceConfig {
    pub handler: CommandHandler,
}

impl ServiceConfig {
    fn service(&self) -> actix_web::Scope {
        web::scope("/api").configure(bitcoin::routes("v1", self.handler.clone()))
    }
}

pub async fn build_http_service(host: &str, service_config: ServiceConfig) -> std::io::Result<()> {
    let process_info: &'static str = Box::leak(
        serde_json::to_string_pretty(atb_cli::process_info())
            .expect("serialize ok. qed")
            .into_boxed_str(),
    );

    let query_cfg = web::QueryConfig::default()
        .error_handler(|err, _| ApiError::ParamsInvalid(err.into()).into());
    let path_cfg = web::PathConfig::default()
        .error_handler(|err, _| ApiError::ParamsInvalid(err.into()).into());
    // Json Deserialize handler
    let json_cfg = web::JsonConfig::default()
        // limit request payload size
        .limit(4096)
        // only accept text/plain content type
        .content_type(|mime| mime == mime::TEXT_PLAIN)
        // use custom error handler
        .error_handler(|err, _| ApiError::ParamsInvalid(err.into()).into());

    HttpServer::new(move || {
        App::new()
            .app_data(query_cfg.clone())
            .app_data(path_cfg.clone())
            .app_data(json_cfg.clone())
            .wrap(Logger::new("From %a %r %s %{Referer}i %D ms"))
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
