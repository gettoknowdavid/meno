//! Notes' contribution to the single error taxonomy (plan §4.2).
//!
//! No `NotesError` enum: the port replaces `master`'s ten-variant module-local enum and
//! its hand-written `IntoResponse` with named constructors over [`MenoError`], the same
//! shape as [`crate::modules::chat::error`] and [`crate::modules::broadcast::error`].
//!
//! # The one thing that is lost, and why it is safe
//!
//! `master`'s `VersionConflict` carried the server's current copy of the row and rendered
//! it as `data` inside a bespoke 409 body. The shared taxonomy carries structured `data`
//! for validation errors only, so a REST 409 here is a plain `VERSION_CONFLICT` and the
//! client refetches. Nothing is lost on the path that actually needs the server copy: the
//! sync engine receives each conflicting row structurally in
//! [`super::dto::MutationResult::server_entity`], which is the only consumer that ever
//! read it.
//!
//! # Ownership is a 403, not a 400
//!
//! `master` answered `NOT_NOTE_OWNER` and `NOT_FOLDER_OWNER` with a `400`. The request is
//! well-formed; the caller simply is not the owner, which is what [`ErrorCode::NotOwner`]
//! and a `403` are for. It also matches [`crate::modules::broadcast::error::not_creator`].

use meno_core::{Error as MenoError, ErrorCode};

pub use meno_core::error::{FieldErrors, invalid_field, invalid_fields};

/// No live note carries this id, or it has been soft-deleted.
#[must_use]
pub fn note_not_found() -> MenoError {
    MenoError::NotFound {
        resource: "note",
        code: ErrorCode::NotFound,
    }
}

/// No live folder carries this id, or it has been soft-deleted.
#[must_use]
pub fn folder_not_found() -> MenoError {
    MenoError::NotFound {
        resource: "folder",
        code: ErrorCode::NotFound,
    }
}

/// The note belongs to somebody else.
#[must_use]
pub fn not_note_owner() -> MenoError {
    MenoError::Forbidden {
        code: ErrorCode::NotOwner,
        message: "You do not own this note".to_owned(),
    }
}

/// The folder belongs to somebody else.
#[must_use]
pub fn not_folder_owner() -> MenoError {
    MenoError::Forbidden {
        code: ErrorCode::NotOwner,
        message: "You do not own this folder".to_owned(),
    }
}

/// The row moved on since the client last read it.
///
/// A `409` with the shared [`ErrorCode::VersionConflict`]; the server's copy is not
/// attached (see the module note). The sync path carries it per-item instead.
#[must_use]
pub fn version_conflict() -> MenoError {
    MenoError::Conflict {
        code: ErrorCode::VersionConflict,
        message: "This item changed elsewhere — review the server copy before retrying".to_owned(),
    }
}

/// A push carried more mutations than one transaction may apply.
#[must_use]
pub fn too_many_mutations(limit: usize) -> MenoError {
    MenoError::BadRequest {
        code: ErrorCode::BadRequest,
        message: format!("Cannot push more than {limit} mutations per request"),
    }
}

/// A driver failure inside the notes module. Always `500`, always logged.
///
/// `context` names the operation and is never user input; `detail` is the driver's own
/// text, which reaches the log and never the client.
#[must_use]
pub fn internal(context: &'static str, detail: impl std::fmt::Display) -> MenoError {
    MenoError::Internal {
        context,
        detail: detail.to_string(),
    }
}

#[cfg(test)]
mod tests {
    //! §4.2's load-bearing properties, asserted on the rendered body.

    use super::*;
    use meno_core::to_body;

    #[test]
    fn every_code_is_one_the_taxonomy_defines() {
        assert_eq!(note_not_found().code(), ErrorCode::NotFound);
        assert_eq!(folder_not_found().code(), ErrorCode::NotFound);
        assert_eq!(not_note_owner().code(), ErrorCode::NotOwner);
        assert_eq!(not_folder_owner().code(), ErrorCode::NotOwner);
        assert_eq!(version_conflict().code(), ErrorCode::VersionConflict);
        assert_eq!(too_many_mutations(200).code(), ErrorCode::BadRequest);
    }

    #[test]
    fn a_refusal_is_a_4xx() {
        assert_eq!(to_body(&note_not_found()).http_status, 404);
        assert_eq!(to_body(&folder_not_found()).http_status, 404);
        assert_eq!(to_body(&not_note_owner()).http_status, 403);
        assert_eq!(to_body(&not_folder_owner()).http_status, 403);
        assert_eq!(to_body(&version_conflict()).http_status, 409);
        assert_eq!(to_body(&too_many_mutations(200)).http_status, 400);
    }

    #[test]
    fn a_note_and_a_folder_are_different_resources() {
        assert_eq!(to_body(&note_not_found()).message, "note not found");
        assert_eq!(to_body(&folder_not_found()).message, "folder not found");
    }

    #[test]
    fn internal_detail_is_logged_never_returned() {
        let error = internal("load_note", "relation \"notes\" does not exist at 10.0.0.4");

        assert_eq!(to_body(&error).http_status, 500);
        assert!(!error.is_client_safe());
        assert!(
            !to_body(&error).message.contains("10.0.0.4"),
            "the address leaked: {}",
            to_body(&error).message
        );
        assert!(error.to_string().contains("load_note"));
    }
}
