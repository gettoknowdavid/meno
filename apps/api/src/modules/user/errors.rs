use crate::shared::errors::{error_response, validation_error_response};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;
use validator::ValidationErrors;

#[derive(Error, Debug)]
pub enum UserError {
    #[error("User not found")]
    NotFound,

    #[error("Avatar must be JPEG or PNG")]
    InvalidFileType,

    #[error("File must be under 5MB")]
    FileTooLarge,

    #[error("Failed to upload image. Please, try again")]
    UploadFailed,

    #[error("Avatar key not found in storage — upload the file first")]
    AvatarNotUploaded,

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Redis(#[from] fred::error::Error),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error("Validation error")]
    ValidationError(#[from] ValidationErrors),
}
impl IntoResponse for UserError {
    fn into_response(self) -> Response {
        match &self {
            UserError::NotFound => {
                error_response(StatusCode::NOT_FOUND, "USER_NOT_FOUND", &self.to_string())
            }
            UserError::InvalidFileType => error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "INVALID_FILE_TYPE",
                &self.to_string(),
            ),
            UserError::FileTooLarge => error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "FILE_TOO_LARGE",
                &self.to_string(),
            ),
            UserError::UploadFailed => {
                tracing::error!("{:?}", self);
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            UserError::AvatarNotUploaded => error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "AVATAR_NOT_UPLOADED",
                &self.to_string(),
            ),
            UserError::StorageError(_)
            | UserError::Database(_)
            | UserError::Redis(_)
            | UserError::Internal(_) => {
                tracing::error!("{:?}", self);
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            UserError::ValidationError(errs) => validation_error_response(errs.clone()),
        }
    }
}
