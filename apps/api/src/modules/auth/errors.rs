use crate::shared::utils::error_response;
use axum::http::StatusCode;
use axum::response::Response;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Token expired")]
    TokenExpired(String),
}

impl AuthError {
    pub fn error_response(error: &AuthError) -> Response {
        match error {
            AuthError::InvalidCredentials => error_response(
                StatusCode::UNAUTHORIZED,
                "INVALID_CREDENTIALS",
                "Invalid credentials",
            ),
            AuthError::UserNotFound(message) => {
                error_response(StatusCode::NOT_FOUND, "NOT_FOUND", message)
            }
            AuthError::TokenExpired(message) => {
                error_response(StatusCode::UNAUTHORIZED, "TOKEN_EXPIRED", message)
            }
        }
    }
}
