use std::fmt::Display;

use actix_web::{http::StatusCode, HttpResponse, ResponseError};
use atb::prelude::thiserror;
use reqwest::header::InvalidHeaderValue;
use serde::Serialize;

type BoxDynError = Box<dyn std::error::Error + 'static + Send + Sync>;

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    /// ===== Sqlx Error =====
    #[error("Sqlx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    /// ===== 400 Series Error =====
    #[error(transparent)]
    Payload(#[from] actix_web::error::PayloadError),

    // EmailVerfied(BoxDynError),
    #[error("Params Invalid: {0}")]
    ParamsInvalid(BoxDynError),

    #[error("Not Found: {0}")]
    NotFound(BoxDynError),

    /// ===== 500 Series Error =====
    #[error("A possible error when converting a `HeaderValue` from a string or byte slice.")]
    ReqwestHeader(#[from] InvalidHeaderValue),

    #[error("Base64 Decode Error: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    #[error("Serde Json Error: {0}")]
    Serde(#[from] serde_json::Error),

    /// ===== Other Error =====
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error("Other Error: {0}")]
    Other(#[source] BoxDynError),
}

// Serialized type from thiserror
#[derive(Serialize, Debug)]
pub struct ErrorResponse {
    tag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

impl<T> From<(&str, Option<T>)> for ErrorResponse
where
    T: Display,
{
    fn from(info: (&str, Option<T>)) -> ErrorResponse {
        ErrorResponse {
            tag: info.0.to_owned(),
            message: info.1.map(|s| format!("{s}")),
        }
    }
}
impl ApiError {
    pub fn info(&self) -> (StatusCode, ErrorResponse) {
        match self {
            // Sqlx Error
            ApiError::Sqlx(e) => match e {
                sqlx::Error::RowNotFound => (
                    StatusCode::NOT_FOUND,
                    ("SQL_ERROR", Some("row not found in database")).into(),
                ),
                sqlx::Error::Decode(_) | sqlx::Error::ColumnDecode { .. } => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ("SQL_ERROR", Some("type decode error from database")).into(),
                ),
                _ => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ("SQL_ERROR", None::<String>).into(),
                ),
            },

            ApiError::Payload(e) => (StatusCode::BAD_REQUEST, ("PAYLOAD_ERROR", Some(e)).into()),

            ApiError::ParamsInvalid(e) => (
                StatusCode::BAD_REQUEST,
                ("INVALID_PARAMS_ERROR", Some(e)).into(),
            ),

            ApiError::NotFound(e) => (StatusCode::NOT_FOUND, ("NOT_FOUND_ERROR", Some(e)).into()),
            // ===== 500 Series Error =====
            ApiError::ReqwestHeader(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ("REQWEST_HEADER_ERROR", None::<String>).into(),
            ),

            ApiError::Base64Decode(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ("BASE64_DECODE_ERROR", None::<String>).into(),
            ),

            ApiError::Serde(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ("SERDE_JSON_ERROR", None::<String>).into(),
            ),

            ApiError::Anyhow(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ("OTHER_ERROR", Some(e)).into(),
            ),
            ApiError::Other(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ("OTHER_ERROR", Some(e)).into(),
            ),
        }
    }
}

// Description in `Google JSON Style Guide`: Container for all the data from a response. This property itself has many reserved property names, which are described below. Services are free to add their own data to this object. A JSON response should contain either a data object or an error object, but not both. If both data and error are present, the error object takes precedence.
// refer: https://google.github.io/styleguide/jsoncstyleguide.xml?showone=data#data
impl ResponseError for ApiError {
    fn error_response(&self) -> HttpResponse {
        let (status_code, err_resp) = self.info();
        // #HACK: log detail error when it's 500 series error
        if status_code.as_u16() >= 400 {
            log::error!("Api Return Error: {err_resp:?}, detail: {self}");
        }
        HttpResponse::build(status_code).json(serde_json::json!({
            "error": err_resp
        }))
    }
}
