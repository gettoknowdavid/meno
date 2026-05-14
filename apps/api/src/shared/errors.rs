use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{borrow::Cow, collections::HashMap};
use thiserror::Error;
use tracing::error;
use validator::{ValidationError, ValidationErrors};

#[derive(Error, Debug)]
pub enum MenoError {
    #[error("{0}")]
    BadRequest(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error("Access denied")]
    Forbidden,

    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error(transparent)]
    Redis(#[from] fred::error::Error),

    #[error("Too many request")]
    TooManyRequests(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Validation error")]
    ValidatorError(#[from] ValidationErrors),
}

impl IntoResponse for MenoError {
    fn into_response(self) -> Response {
        match &self {
            MenoError::BadRequest(msg) => {
                error_response(StatusCode::BAD_REQUEST, "BAD_REQUEST", msg)
            }
            MenoError::Conflict(msg) => error_response(StatusCode::CONFLICT, "CONFLICT", msg),
            MenoError::Database(_) => {
                error!("{:?}", self);
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            MenoError::Forbidden => {
                error_response(StatusCode::FORBIDDEN, "FORBIDDEN", "Access denied")
            }
            MenoError::Internal(_) => {
                error!("{:?}", self);
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            MenoError::NotFound(msg) => error_response(StatusCode::NOT_FOUND, "NOT_FOUND", msg),
            MenoError::Redis(_) => {
                error!("{:?}", self);
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            MenoError::TooManyRequests(msg) => {
                error_response(StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_REQUESTS", msg)
            }
            MenoError::Unauthorized(msg) => {
                error_response(StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg)
            }
            MenoError::ValidatorError(err) => validation_error_response(err.clone()),
        }
    }
}
pub fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    let body = axum::Json(serde_json::json!({
        "data": null,
        "meta": null,
        "error": {
            "code": code,
            "message": message,
        }
    }));
    (status, body).into_response()
}
fn validation_error_response(errs: ValidationErrors) -> Response {
    fn extract(arg: (&Cow<str>, &&Vec<ValidationError>)) -> (String, Vec<String>) {
        let messages = arg
            .1
            .iter()
            .map(|e| e.message.as_deref().unwrap_or("Invalid value").to_string())
            .collect();
        (arg.0.to_string(), messages)
    }
    let error_map: HashMap<String, Vec<String>> = errs.field_errors().iter().map(extract).collect();
    let body = axum::Json(serde_json::json!({
        "data": null,
        "meta": null,
        "error": {
            "code": "VALIDATION_ERROR",
            "message": "One or more fields are invalid",
            "errors": error_map,
        }
    }));
    (StatusCode::BAD_REQUEST, body).into_response()
}
