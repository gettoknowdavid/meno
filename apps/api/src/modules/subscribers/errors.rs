use crate::shared::errors::error_response;
use crate::shared::pagination::CursorError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SubscribersError {
    #[error("You cannot subscribe to yourself.")]
    CannotSubscribeToSelf,

    #[error("Subscriber not found")]
    SubscriberNotFound,

    #[error("Subscription not found")]
    SubscriptionNotFound,

    #[error("Cursor error: {0}")]
    Cursor(#[from] CursorError),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Redis(#[from] fred::error::Error),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
impl IntoResponse for SubscribersError {
    fn into_response(self) -> Response {
        match &self {
            SubscribersError::CannotSubscribeToSelf => error_response(
                StatusCode::BAD_REQUEST,
                "CANNOT_SUBSCRIBE_TO_SELF",
                &self.to_string(),
            ),
            SubscribersError::SubscriberNotFound => error_response(
                StatusCode::NOT_FOUND,
                "SUBSCRIBER_NOT_FOUND",
                &self.to_string(),
            ),
            SubscribersError::SubscriptionNotFound => error_response(
                StatusCode::NOT_FOUND,
                "SUBSCRIPTION_NOT_FOUND",
                &self.to_string(),
            ),
            SubscribersError::Database(e) => {
                tracing::error!(
                    error.kind = "database",
                    // Don't log the full SQL error in prod (may contain data)
                    // Use error chain for correlation
                    error.message = %e,
                    "database error in subscribers handler"
                );
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            SubscribersError::Redis(e) => {
                tracing::error!(error.kind = "redis", error.message = %e, "redis error in subscribers handler");
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            SubscribersError::Internal(e) => {
                tracing::error!(error.kind = "internal", error.message = %e, "unhandled internal error");
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            SubscribersError::Cursor(e) => {
                tracing::error!(error.kind = "cursor", error.message = %e, "cursor error in subscribers handler");
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
        }
    }
}
