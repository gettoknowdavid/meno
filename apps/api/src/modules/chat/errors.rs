use crate::shared::errors::error_response;
use crate::shared::pagination::CursorError;
use crate::shared::services::redis::coalescing::CacheError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;
use validator::ValidationErrors;

#[derive(Error, Debug)]
pub enum ChatError {
    #[error("Message editing window has expired")]
    EditWindowExpired,

    #[error("Message not found")]
    NotFound,

    #[error("Broadcast not found")]
    BroadcastNotFound,

    #[error("Broadcast is not currently live")]
    BroadcastNotLive,

    #[error("You are not a participant in this broadcast")]
    NotParticipant,

    #[error("You can only edit your own messages")]
    NotSender,

    #[error("You cannot send a message to yourself.")]
    CannotSendMessageToSelf,

    #[error("User not found")]
    UserNotFound,

    #[error("Cursor error: {0}")]
    Cursor(#[from] CursorError),

    #[error("Cache error: {0}")]
    Cache(#[from] CacheError),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Redis(#[from] fred::error::Error),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error("Validation error")]
    Validation(#[from] ValidationErrors),
}
impl IntoResponse for ChatError {
    fn into_response(self) -> Response {
        match &self {
            ChatError::EditWindowExpired => error_response(
                StatusCode::BAD_REQUEST,
                "EDIT_WINDOW_EXPIRED",
                &self.to_string(),
            ),
            ChatError::NotFound => error_response(
                StatusCode::NOT_FOUND,
                "MESSAGE_NOT_FOUND",
                &self.to_string(),
            ),
            ChatError::BroadcastNotFound => error_response(
                StatusCode::NOT_FOUND,
                "BROADCAST_NOT_FOUND",
                &self.to_string(),
            ),
            ChatError::NotParticipant => error_response(
                StatusCode::FORBIDDEN,
                "NOT_BROADCAST_PARTICIPANT",
                &self.to_string(),
            ),
            ChatError::NotSender => error_response(
                StatusCode::FORBIDDEN,
                "NOT_MESSAGE_SENDER",
                &self.to_string(),
            ),
            ChatError::BroadcastNotLive => error_response(
                StatusCode::BAD_REQUEST,
                "BROADCAST_NOT_LIVE",
                &self.to_string(),
            ),
            ChatError::CannotSendMessageToSelf => error_response(
                StatusCode::BAD_REQUEST,
                "CANNOT_SEND_MESSAGE_TO_SELF",
                &self.to_string(),
            ),
            ChatError::UserNotFound => {
                error_response(StatusCode::NOT_FOUND, "USER_NOT_FOUND", &self.to_string())
            }
            ChatError::Database(e) => {
                tracing::error!(
                    error.kind = "database",
                    error.message = %e,
                    "database error in chat handler"
                );
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            ChatError::Redis(e) => {
                tracing::error!(error.kind = "redis", error.message = %e, "redis error in chat handler");
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            ChatError::Cache(e) => {
                tracing::error!(error.kind = "cache", error.message = %e, "cache error in chat handler");
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            ChatError::Internal(e) => {
                tracing::error!(error.kind = "internal", error.message = %e, "unhandled internal error");
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            ChatError::Cursor(e) => {
                tracing::error!(error.kind = "cursor", error.message = %e, "cursor error in chat handler");
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            ChatError::Validation(errs) => {
                crate::shared::errors::validation_error_response(errs.clone())
            }
        }
    }
}
