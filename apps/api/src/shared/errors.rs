use crate::modules::auth::errors::AuthError;
use crate::shared::utils::error_response;

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
    #[error("Auth error: {0}")]
    Auth(#[from] AuthError),

    #[error("Bad request")]
    BadRequest(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Access Denied.")]
    Forbidden,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Redis error")]
    Redis(#[from] fred::error::Error),

    #[error("Too many request")]
    TooManyRequests(String),

    #[error("Validation error")]
    ValidatorError(ValidationErrors),
}

impl From<ValidationErrors> for MenoError {
    fn from(err: ValidationErrors) -> Self {
        MenoError::ValidatorError(err)
    }
}

impl IntoResponse for MenoError {
    fn into_response(self) -> Response {
        match &self {
            MenoError::Auth(e) => AuthError::error_response(e),
            MenoError::BadRequest(message) => {
                error_response(StatusCode::BAD_REQUEST, "BAD_REQUEST", message)
            }
            MenoError::Database(_) => {
                error!("Database error: {:?}", self);
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
                error!("Internal error: {:?}", self);
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            MenoError::Redis(_) => {
                error!("Redis error: {:?}", self);
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            MenoError::TooManyRequests(message) => {
                error_response(StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_REQUESTS", message)
            }
            MenoError::ValidatorError(err) => {
                error!("Validation error: {:?}", self);
                validation_error_response(err.clone())
            }
        }
    }
}

fn validation_error_response(validation_errors: ValidationErrors) -> Response {
    fn get_message(arg: (&Cow<str>, &&Vec<ValidationError>)) -> (String, Vec<String>) {
        let messages = arg
            .1
            .iter()
            .map(|e| e.message.as_deref().unwrap_or("Invalid value").to_string())
            .collect();
        (arg.0.to_string(), messages)
    }

    let error_map: HashMap<String, Vec<String>> = validation_errors
        .field_errors()
        .iter()
        .map(get_message)
        .collect();

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
