use crate::shared::errors::{error_response, validation_error_response};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;
use validator::ValidationErrors;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Email already in use")]
    EmailTaken,

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Account registered with Google — use Google sign-in")]
    GoogleAccountConflict,

    #[error("Email not verified — check your inbox")]
    EmailNotVerified,

    #[error("Invalid or expired OTP")]
    InvalidOtp,

    #[error("OTP already used")]
    OtpAlreadyUsed,

    #[error("Token expired — please sign in again")]
    AccessTokenExpired,

    #[error("Refresh token expired — please sign in again")]
    RefreshTokenExpired,

    #[error("Invalid token")]
    InvalidToken,

    #[error("Missing or invalid token")]
    MissingToken,

    #[error("Token not found — please sign in again")]
    RefreshTokenNotFound,

    #[error("Failed to create token")]
    TokenCreationFailed,

    #[error("User not found")]
    UserNotFound,

    #[error("Google authentication failed: {0}")]
    GoogleAuthFailed(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error("Failed to hash password")]
    PasswordHash,

    #[error("Validation error")]
    ValidationError(#[from] ValidationErrors),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        match &self {
            AuthError::EmailTaken => {
                error_response(StatusCode::CONFLICT, "EMAIL_TAKEN", &self.to_string())
            }
            AuthError::InvalidCredentials => error_response(
                StatusCode::UNAUTHORIZED,
                "INVALID_CREDENTIALS",
                &self.to_string(),
            ),
            AuthError::GoogleAccountConflict => error_response(
                StatusCode::CONFLICT,
                "GOOGLE_ACCOUNT_CONFLICT",
                &self.to_string(),
            ),
            AuthError::EmailNotVerified => error_response(
                StatusCode::FORBIDDEN,
                "EMAIL_NOT_VERIFIED",
                &self.to_string(),
            ),
            AuthError::InvalidOtp => {
                error_response(StatusCode::BAD_REQUEST, "INVALID_OTP", &self.to_string())
            }
            AuthError::OtpAlreadyUsed => error_response(
                StatusCode::BAD_REQUEST,
                "OTP_ALREADY_USED",
                &self.to_string(),
            ),
            AuthError::AccessTokenExpired => error_response(
                StatusCode::UNAUTHORIZED,
                "ACCESS_TOKEN_EXPIRED",
                &self.to_string(),
            ),
            AuthError::RefreshTokenExpired => error_response(
                StatusCode::UNAUTHORIZED,
                "REFRESH_TOKEN_EXPIRED",
                &self.to_string(),
            ),
            AuthError::InvalidToken => {
                error_response(StatusCode::UNAUTHORIZED, "INVALID_TOKEN", &self.to_string())
            }
            AuthError::MissingToken => {
                error_response(StatusCode::UNAUTHORIZED, "MISSING_TOKEN", &self.to_string())
            }
            AuthError::RefreshTokenNotFound => error_response(
                StatusCode::UNAUTHORIZED,
                "REFRESH_TOKEN_NOT_FOUND",
                &self.to_string(),
            ),
            AuthError::TokenCreationFailed => error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "TOKEN_CREATION_FAILED",
                &self.to_string(),
            ),
            AuthError::UserNotFound => {
                error_response(StatusCode::NOT_FOUND, "USER_NOT_FOUND", &self.to_string())
            }
            AuthError::GoogleAuthFailed(_) => error_response(
                StatusCode::UNAUTHORIZED,
                "GOOGLE_AUTH_FAILED",
                &self.to_string(),
            ),
            AuthError::Database(_) | AuthError::Internal(_) => {
                tracing::error!("{:?}", self);
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            AuthError::PasswordHash => error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "An internal error occurred",
            ),
            AuthError::ValidationError(errs) => validation_error_response(errs.clone()),
        }
    }
}
