//! The single response envelope for every endpoint.
//!
//! Copied from `apps/api/src/shared/types/meno_response.rs` on `master` (`903c3ba`) with
//! one behavioural change, recorded below.
//!
//! # Why this lives in `apps/api`, not `crates/core`
//!
//! `crates/core` has a hard constraint: no `axum`, no `std::net`, no I/O of any kind
//! (plan §2.1). This type implements `IntoResponse`, which means `axum::Json` and
//! `axum::http::StatusCode`. It therefore cannot move to `core` without breaking that
//! crate's central property — the one that lets its logic be tested with no Docker.
//!
//! What *is* pure and does live in `core` is [`meno_core::ErrorBody`], the status/code/
//! message triple for failures. `core` cannot know how a success envelope is shaped, so
//! the split is: `core` owns the error contract, `apps/api` owns the HTTP rendering of
//! both.
//!
//! # Wire contract
//!
//! ```json
//! { "status_code": 200, "code": "OK", "message": "…", "status": true, "data": { } }
//! ```
//!
//! Field names are snake_case and are **not** renamed here. Flutter and Next.js parse
//! this exact shape; `data` nests payloads that *do* use camelCase (`CursorPage` emits
//! `nextCursor`/`hasNextPage`), so the two conventions apply at different levels and
//! neither is a mistake.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// The single response shape for every endpoint in the app.
///
/// Matches the `NestJS` contract your clients (Flutter, Next.js) are built against:
/// `{ code, message, status, data? }`
///
/// `data` is `Option<T>`, so endpoints that return nothing (logout, delete, mark-read)
/// can use [`MenoResponse::no_content`] and `data` will be omitted from the JSON.
#[derive(Debug, Serialize)]
pub struct MenoResponse<T: Serialize> {
    /// The HTTP status, duplicated in the body so a client reading JSON only still sees
    /// it. Mirrors [`meno_core::ErrorBody::status_code`].
    pub status_code: u16,

    /// Stable, machine-readable code. See [the stability note below](#code-stability).
    pub code: String,

    /// Human-readable message. May be reworded at any time; clients must not match on it.
    pub message: String,

    /// `true` on success. Always `false` in [`meno_core::ErrorBody`].
    pub status: bool,

    /// The payload. Omitted entirely from the JSON when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

/// # Code stability
///
/// On `master` the success paths set `code` to `StatusCode::OK.to_string()` — the string
/// `"200 OK"` — while the error paths already use stable codes like `"BAD_REQUEST"`. So
/// the same field carried two different conventions depending on whether the request
/// succeeded, and a client could not branch on it uniformly.
///
/// Plan §4.2 requires *"a stable machine-readable `code` so the Flutter and Next.js
/// clients never string-match on human-readable messages"*, so success now uses `"OK"`
/// and `"CREATED"`. These match the HTTP reason phrases, but unlike `"200 OK"` they are
/// a deliberate, versioned contract rather than a rendering of the status line.
pub const CODE_OK: &str = "OK";

/// Stable code returned by [`MenoResponse::created`].
pub const CODE_CREATED: &str = "CREATED";

impl<T: Serialize> MenoResponse<T> {
    /// 200 with a payload.
    #[must_use]
    pub fn ok(message: impl Into<String>, data: T) -> Self {
        Self {
            status_code: StatusCode::OK.as_u16(),
            code: CODE_OK.to_string(),
            message: message.into(),
            status: true,
            data: Some(data),
        }
    }

    /// 201 with the created resource as the payload.
    #[must_use]
    pub fn created(message: impl Into<String>, data: T) -> Self {
        Self {
            status_code: StatusCode::CREATED.as_u16(),
            code: CODE_CREATED.to_string(),
            message: message.into(),
            status: true,
            data: Some(data),
        }
    }
}

impl MenoResponse<()> {
    /// Success with no payload — logout, delete, mark-read, leave broadcast, etc.
    /// `data` is omitted entirely from the JSON (`skip_serializing_if` = `None`).
    #[must_use]
    pub fn no_content(message: impl Into<String>) -> Self {
        Self {
            status_code: StatusCode::OK.as_u16(),
            code: CODE_OK.to_string(),
            message: message.into(),
            status: true,
            data: None,
        }
    }
}

impl<T: Serialize> IntoResponse for MenoResponse<T> {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(self),
        )
            .into_response()
    }
}
