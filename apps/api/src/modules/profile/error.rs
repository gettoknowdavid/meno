//! §4.2 constructors for the profile module.
//!
//! There is no `ProfileError` enum: plan §4.2 retires the "nine independent error
//! enums" pattern, and this module is instead the set of named ways profile builds the
//! one [`meno_core::Error`] — the shape `auth::error` established and `broadcast::error`
//! repeats. A constructor rather than a variant, because it can default the parts that
//! are always the same: the status, the wire code, and the "never leak internals" rule
//! that turns a driver error into `500` plus a log line.
//!
//! # What a client can rely on
//!
//! Every failure below is one of the §4.2 taxonomy's variants, so the wire codes are
//! `NOT_FOUND`, `VALIDATION_FAILED`, `UPSTREAM_UNAVAILABLE` and `INTERNAL_ERROR`. The
//! old module shipped profile-private codes (`INVALID_AVATAR_KEY`, `FILE_TOO_LARGE`,
//! `INVALID_FILE_TYPE`); those are `Validation` field errors now — a file that is too
//! large *is* a field that failed a rule, and one taxonomy means the client branches on
//! `VALIDATION_FAILED` with a `file` entry rather than on three bespoke strings.
//!
//! # What is logged vs sent
//!
//! `internal` and `upstream` carry a `detail` that is logged and never returned: the
//! client gets the generic message, the operator gets the driver text.

use std::collections::HashMap;

use meno_core::{Error as MenoError, ErrorCode};

/// Re-exported rather than imported ad hoc: §4.2's rule is one taxonomy with one
/// accumulator, so [`meno_core::error::FieldErrors`] is what `dto::validate` fills
/// everywhere — see the same re-export in `auth::error`.
pub use meno_core::error::FieldErrors;

/// One message against one field — the §4.2 validation shape without an accumulator.
fn field_error(field: &str, message: String) -> MenoError {
    MenoError::Validation {
        fields: HashMap::from([(field.to_owned(), vec![message])]),
    }
}

/// The profile does not exist (or is soft-deleted). 404 `NOT_FOUND`.
#[must_use]
pub fn not_found() -> MenoError {
    MenoError::NotFound {
        resource: "profile",
        code: ErrorCode::NotFound,
    }
}

/// A bug or an unreachable dependency. 500 with a generic message; `detail` is logged.
#[must_use]
pub fn internal(context: &'static str, detail: impl std::fmt::Display) -> MenoError {
    MenoError::Internal {
        context,
        detail: detail.to_string(),
    }
}

/// The object store refused or failed the write. 503 `UPSTREAM_UNAVAILABLE` —
/// also what `STORAGE_ENABLED=false` (`NoopObjectStore`) produces, because a disabled
/// upload path is a deployment condition, not a client mistake.
#[must_use]
pub fn upstream(detail: impl std::fmt::Display) -> MenoError {
    MenoError::Upstream {
        service: "storage",
        detail: detail.to_string(),
    }
}

/// The upload exceeds `MAX_AVATAR_BYTES`. 422, field `file`.
#[must_use]
pub fn avatar_too_large(limit_bytes: usize) -> MenoError {
    field_error("file", format!("must be at most {limit_bytes} bytes"))
}

/// The bytes are not a JPEG, PNG or WebP. 422, field `file`.
#[must_use]
pub fn avatar_not_an_image() -> MenoError {
    field_error("file", "must be a JPEG, PNG or WebP image".to_owned())
}

/// The multipart body carried no file part named `file`. 422, field `file`.
#[must_use]
pub fn avatar_part_missing() -> MenoError {
    field_error("file", "a file part named `file` is required".to_owned())
}

/// The file part was present but empty — a client bug worth catching before a
/// zero-byte object is written. 422, field `file`.
#[must_use]
pub fn avatar_empty() -> MenoError {
    field_error("file", "the uploaded file is empty".to_owned())
}
