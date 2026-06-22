use crate::shared::errors::{error_response, validation_error_response};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(thiserror::Error, Debug)]
pub enum SettingsError {
    #[error("Settings not found")]
    NotFound,

    #[error("Unsupported language code")]
    InvalidLanguage,

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error("Validation error")]
    ValidationError(#[from] validator::ValidationErrors),
}

impl IntoResponse for SettingsError {
    fn into_response(self) -> Response {
        match &self {
            SettingsError::NotFound => error_response(
                StatusCode::NOT_FOUND,
                "SETTINGS_NOT_FOUND",
                &self.to_string(),
            ),
            SettingsError::InvalidLanguage => error_response(
                StatusCode::BAD_REQUEST,
                "INVALID_LANGUAGE",
                &self.to_string(),
            ),
            SettingsError::Database(e) => {
                tracing::error!(error.kind = "database", error.message = %e, "database error in settings handler");
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            SettingsError::ValidationError(errs) => validation_error_response(errs.clone()),
        }
    }
}
