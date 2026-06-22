use axum::response::IntoResponse;

#[derive(thiserror::Error, Debug)]
pub enum NotesError {
    #[error("Note not found")]
    NoteNotFound,

    #[error("Folder not found")]
    FolderNotFound,

    #[error("{0}")]
    BadRequest(String),

    #[error("This item changed elsewhere")]
    VersionConflict(crate::modules::notes::dto::ConflictEntity),

    #[error("You are not the owner of this note")]
    NotNoteOwner,

    #[error("You are not the owner of this folder")]
    NotFolderOwner,

    #[error(transparent)]
    Cursor(#[from] crate::shared::pagination::CursorError),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error("Validation error")]
    ValidationError(#[from] validator::ValidationErrors),
}
impl IntoResponse for NotesError {
    fn into_response(self) -> axum::response::Response {
        match &self {
            NotesError::NoteNotFound => crate::shared::errors::error_response(
                axum::http::StatusCode::NOT_FOUND,
                "NOTE_NOT_FOUND",
                &self.to_string(),
            ),
            NotesError::FolderNotFound => crate::shared::errors::error_response(
                axum::http::StatusCode::NOT_FOUND,
                "FOLDER_NOT_FOUND",
                &self.to_string(),
            ),
            NotesError::BadRequest(msg) => crate::shared::errors::error_response(
                axum::http::StatusCode::BAD_REQUEST,
                "BAD_REQUEST",
                msg,
            ),
            NotesError::VersionConflict(entity) => version_conflict_response(entity),
            NotesError::NotNoteOwner => crate::shared::errors::error_response(
                axum::http::StatusCode::BAD_REQUEST,
                "NOT_NOTE_OWNER",
                &self.to_string(),
            ),
            NotesError::NotFolderOwner => crate::shared::errors::error_response(
                axum::http::StatusCode::BAD_REQUEST,
                "NOT_FOLDER_OWNER",
                &self.to_string(),
            ),
            NotesError::Cursor(e) => {
                tracing::error!(error.kind = "cursor", error.message = %e, "cursor error in notes handler");
                crate::shared::errors::error_response(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            NotesError::Database(e) => {
                tracing::error!(error.kind = "database",error.message = %e,"database error in notes handler");
                crate::shared::errors::error_response(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            NotesError::Internal(e) => {
                tracing::error!(error.kind = "internal", error.message = %e, "unhandled internal error");
                crate::shared::errors::error_response(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            NotesError::ValidationError(errs) => {
                crate::shared::errors::validation_error_response(errs.clone())
            }
        }
    }
}

/// Same shape as `validation_error_response` — a dedicated function because
/// `error_response()` always hardcodes `data: None`, and a 409 here needs to
/// carry the server's current copy of the conflicting row.
fn version_conflict_response(
    entity: &crate::modules::notes::dto::ConflictEntity,
) -> axum::response::Response {
    let body = axum::Json(crate::shared::types::meno_response::MenoResponse {
        status_code: axum::http::StatusCode::CONFLICT.as_u16(),
        code: "VERSION_CONFLICT".to_string(),
        message: "This item changed elsewhere — review the server copy before retrying".to_string(),
        status: false,
        data: Some(entity.clone()),
    });
    (axum::http::StatusCode::CONFLICT, body).into_response()
}
