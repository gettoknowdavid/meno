//! §4.2 constructors for the settings module.
//!
//! Same shape as `auth::error` and `profile::error`: no enum, no `IntoResponse` —
//! just the named ways settings builds the one [`meno_core::Error`]. `master`'s
//! `SettingsError` carried its own `NotFound` / `InvalidLanguage` / `Database` /
//! `ValidationError` variants with a per-variant status and code; here those are
//! `NOT_FOUND`, `VALIDATION_FAILED` (as field errors) and `INTERNAL_ERROR`, and the
//! mapping lives in exactly one place — `crate::middleware::from_error`.
//!
//! # Why there is no `not_found` here
//!
//! A settings row is created lazily by the first `PATCH` (nothing inserts one at
//! signup — `grep "INSERT INTO settings"` finds no code path). So a missing row is
//! not an error this module can observe: `GET` answers with [`super::model::Settings::new`]'s
//! defaults, `PATCH` upserts, and clearing a token that was never set is a no-op
//! success. 404 would mean "this user has no settings", which is not a state a client
//! can fix or care about.

use std::collections::HashMap;

use meno_core::Error as MenoError;

/// Re-exported so `dto::validate` has one accumulator to fill, exactly as
/// `auth::error` and `profile::error` do.
pub use meno_core::error::FieldErrors;

/// One message against one field — the §4.2 validation shape without an accumulator.
fn field_error(field: &str, message: String) -> MenoError {
    MenoError::Validation {
        fields: HashMap::from([(field.to_owned(), vec![message])]),
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

/// The language tag is not one this API speaks. 422, field `language`.
///
/// Quoted as an example rather than as a spec: the rule itself lives in
/// [`super::dto::is_language_tag`], and the message and the rule are allowed to
/// drift only in wording, never in what they accept.
#[must_use]
pub fn language_invalid() -> MenoError {
    field_error(
        "language",
        "must be a language tag like `en` or `en-GB`".to_owned(),
    )
}

/// The device token exceeds the FCM limit. 422, field `push_notification_token`.
#[must_use]
pub fn push_token_too_long(max_chars: usize) -> MenoError {
    field_error(
        "push_notification_token",
        format!("must be at most {max_chars} characters"),
    )
}
