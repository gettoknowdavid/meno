use crate::shared::errors::error_response;
use crate::shared::pagination::CursorError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NotificationError {
    #[error("Notification not found")]
    NotFound,

    #[error("Cursor error: {0}")]
    Cursor(#[from] CursorError),

    #[error("Notification template '{0}' not found")]
    TemplateNotFound(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Redis(#[from] fred::error::Error),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for NotificationError {
    fn into_response(self) -> Response {
        match &self {
            NotificationError::NotFound => error_response(
                StatusCode::NOT_FOUND,
                "NOTIFICATION_NOT_FOUND",
                &self.to_string(),
            ),
            NotificationError::TemplateNotFound(_) => {
                tracing::error!(error.message = %self, "notification template missing");
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            NotificationError::Database(e) => {
                tracing::error!(
                    error.kind = "database",
                    error.message = %e,
                    "database error in notifications handler"
                );
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            NotificationError::Redis(e) => {
                tracing::error!(
                    error.kind = "redis",
                    error.message = %e,
                    "redis error in notifications handler"
                );
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            NotificationError::Internal(e) => {
                tracing::error!(
                    error.kind = "internal",
                    error.message = %e,
                    "unhandled internal error in notifications"
                );
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            NotificationError::Cursor(e) => {
                tracing::error!(
                    error.kind = "cursor",
                    error.message = %e,
                    "cursor error in notifications handler"
                );
                error_response(
                    StatusCode::BAD_REQUEST,
                    "INVALID_CURSOR",
                    "Invalid pagination cursor",
                )
            }
        }
    }
}
