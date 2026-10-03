//! The single error taxonomy for the whole Meno backend.
//!
//! This module is the consolidation of the nine independent error enums that existed on
//! `master` (`MenoError`, `AuthError`, `BroadcastError`, `ChatError`, `NotesError`,
//! `NotificationError`, `ProfileError`, `SettingsError`, `SubscriberError`), whose
//! duplicated — and drifting — `IntoResponse` mappings are called out in §4.2.
//!
//! **No driver types live here.** §4.2's sketch carries `sqlx::Error` and
//! `fred::error::Error` in variants, which would drag both into `crates/core` and
//! violate the §2.1 purity constraint. Instead, infrastructure failures are erased to a
//! string at the repository boundary by `infrastructure::database::db_err`, which logs
//! the real driver error and returns `Error::Internal`. That is the §5.5 fix, and it is
//! what allows this type to depend on nothing but `thiserror` and `serde`.
//!
//! Consequently there is deliberately no `impl From<sqlx::Error> for Error` — adding
//! one would reintroduce the leak through the back door.

use std::collections::HashMap;

/// Stable, machine-readable error codes.
///
/// # Stability contract
///
/// Every variant is a wire value. Flutter and Next.js branch on these strings. The
/// human-readable `message` may be reworded at any time; a `code` may not be renamed or
/// removed without an API version bump. Adding codes is the intended extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorCode {
    // 400
    /// Malformed request that has no more specific code.
    BadRequest,
    /// The `?cursor=` value could not be decoded.
    InvalidCursor,
    /// The supplied IANA time zone is unknown.
    InvalidTimeZone,
    /// A broadcast was scheduled in the past.
    StartTimeInPast,
    // 401
    /// No credentials, or credentials that do not identify anyone.
    Unauthorized,
    /// Email/password pair rejected.
    InvalidCredentials,
    /// Token structurally invalid — malformed, bad signature, or wrong algorithm.
    InvalidToken,
    /// Access token past its expiry.
    TokenExpired,
    /// Refresh token past its expiry; the session must be re-established.
    RefreshTokenExpired,
    // 403
    /// Authenticated, but not permitted.
    Forbidden,
    /// Caller is not the creator of the resource.
    NotCreator,
    /// Caller does not own the resource.
    NotOwner,
    /// Caller has not joined the room.
    NotParticipant,
    // 404
    /// The resource does not exist, or is soft-deleted.
    NotFound,
    // 409
    /// Generic state conflict.
    Conflict,
    /// Registration rejected because the email is already in use.
    EmailTaken,
    /// The entity already exists.
    AlreadyExists,
    /// Optimistic-concurrency check failed — the client's `version` is stale.
    VersionConflict,
    // 422
    /// One or more fields failed validation; see `ErrorBody::data`.
    ValidationFailed,
    // 429
    /// Rate limit exceeded; see `Error::RateLimited`.
    RateLimited,
    // 503
    /// A dependency (LiveKit, Brevo, FCM, R2) failed or is disabled.
    UpstreamUnavailable,
    // 500
    /// An internal failure. The detail is logged, never returned.
    Internal,
}

impl ErrorCode {
    /// The exact string sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BadRequest => "BAD_REQUEST",
            Self::InvalidCursor => "INVALID_CURSOR",
            Self::InvalidTimeZone => "INVALID_TIME_ZONE",
            Self::StartTimeInPast => "START_TIME_IN_PAST",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::InvalidCredentials => "INVALID_CREDENTIALS",
            Self::InvalidToken => "INVALID_TOKEN",
            Self::TokenExpired => "TOKEN_EXPIRED",
            Self::RefreshTokenExpired => "REFRESH_TOKEN_EXPIRED",
            Self::Forbidden => "FORBIDDEN",
            Self::NotCreator => "NOT_CREATOR",
            Self::NotOwner => "NOT_OWNER",
            Self::NotParticipant => "NOT_PARTICIPANT",
            Self::NotFound => "NOT_FOUND",
            Self::Conflict => "CONFLICT",
            Self::EmailTaken => "EMAIL_TAKEN",
            Self::AlreadyExists => "ALREADY_EXISTS",
            Self::VersionConflict => "VERSION_CONFLICT",
            Self::ValidationFailed => "VALIDATION_FAILED",
            Self::RateLimited => "RATE_LIMITED",
            Self::UpstreamUnavailable => "UPSTREAM_UNAVAILABLE",
            Self::Internal => "INTERNAL_ERROR",
        }
    }
}

/// The single error type for the whole API.
///
/// Infrastructure failures are erased to a string at the repository boundary and logged
/// there with the driver error. Nothing driver-shaped crosses into domain code — that is
/// the §5.5 fix, and it is also what lets this type live with no `sqlx` and no `fred`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 400 — the request is malformed and the caller must change it.
    #[error("{message}")]
    BadRequest {
        /// Stable wire code, e.g. [`ErrorCode::InvalidCursor`].
        code: ErrorCode,
        /// Human-readable detail. Safe to show the client.
        message: String,
    },
    /// 401 — authentication failed. The client should re-authenticate.
    #[error("{message}")]
    Unauthorized {
        /// Stable wire code, e.g. [`ErrorCode::TokenExpired`].
        code: ErrorCode,
        /// Human-readable detail. Safe to show the client.
        message: String,
    },
    /// 403 — authenticated but not permitted.
    #[error("{message}")]
    Forbidden {
        /// Stable wire code, e.g. [`ErrorCode::NotCreator`].
        code: ErrorCode,
        /// Human-readable detail. Safe to show the client.
        message: String,
    },
    /// 404 — the resource does not exist, or is soft-deleted.
    ///
    /// `resource` is a static string like `"broadcast"` — never user input.
    #[error("{resource} not found")]
    NotFound {
        /// The kind of resource, e.g. `"broadcast"`. Never user input.
        resource: &'static str,
        /// Stable wire code; [`ErrorCode::NotFound`] in practice.
        code: ErrorCode,
    },
    /// 409 — the request is valid but conflicts with current state.
    #[error("{message}")]
    Conflict {
        /// Stable wire code, e.g. [`ErrorCode::EmailTaken`].
        code: ErrorCode,
        /// Human-readable detail. Safe to show the client.
        message: String,
    },
    /// 422 — field-level validation failed; the map is returned to the client as `data`.
    #[error("validation failed")]
    Validation {
        /// Field name to the messages that failed for it.
        fields: HashMap<String, Vec<String>>,
    },
    /// 429 — rate limit exceeded.
    #[error("rate limited")]
    RateLimited {
        /// Seconds the client should wait before retrying.
        retry_after_secs: u64,
    },
    /// 503 — an upstream (LiveKit, Brevo, FCM, R2) failed or is disabled.
    #[error("{service} unavailable")]
    Upstream {
        /// The dependency name, e.g. `"livekit"`. Never a URL.
        service: &'static str,
        /// The upstream's own error text. Logged, never returned.
        detail: String,
    },
    /// 500 — an internal failure.
    ///
    /// Always logged at `error` with full context; the client gets a generic message.
    #[error("{context}: {detail}")]
    Internal {
        /// Where it happened, e.g. `"load_broadcast"`. Never user input.
        context: &'static str,
        /// The underlying detail. Logged, never returned.
        detail: String,
    },
}

impl Error {
    /// The stable wire code for this error.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::BadRequest { code, .. }
            | Self::Unauthorized { code, .. }
            | Self::Forbidden { code, .. }
            | Self::NotFound { code, .. }
            | Self::Conflict { code, .. } => *code,
            Self::Validation { .. } => ErrorCode::ValidationFailed,
            Self::RateLimited { .. } => ErrorCode::RateLimited,
            Self::Upstream { .. } => ErrorCode::UpstreamUnavailable,
            Self::Internal { .. } => ErrorCode::Internal,
        }
    }

    /// True when the message is the caller's to see. Everything else must be logged and
    /// replaced with a generic message.
    #[must_use]
    pub const fn is_client_safe(&self) -> bool {
        matches!(
            self,
            Self::BadRequest { .. }
                | Self::Unauthorized { .. }
                | Self::Forbidden { .. }
                | Self::NotFound { .. }
                | Self::Conflict { .. }
                | Self::Validation { .. }
                | Self::RateLimited { .. }
        )
    }
}

/// The status/code/message triple sent to the client. Pure data — no `axum`, no
/// `http::StatusCode`. `apps/api` renders it.
///
/// **No `rename_all` here, deliberately.** `ErrorBody` *is* the response envelope for
/// errors, so its field names are the wire contract that Flutter and Next.js are built
/// against: `status_code`, not `statusCode`. `MenoResponse` on `master` carries no
/// `rename_all` and therefore serialises `status_code`; applying `camelCase` here would
/// make errors and successes use different field names for the same concept, and the
/// camelCase convention would apply only to the nested payload inside `data`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ErrorBody {
    /// The HTTP status code, duplicated in the body so clients reading JSON only still
    /// see it.
    pub status_code: u16,
    /// The stable wire code from [`ErrorCode::as_str`].
    pub code: &'static str,
    /// Human-readable message. Always generic for non-client-safe errors.
    pub message: String,
    /// Always `false`; mirrors the success envelope's `status` field.
    pub status: bool,
    /// Per-field detail. Present only for [`Error::Validation`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<HashMap<String, Vec<String>>>,
}

/// Map an `Error` to its HTTP status, wire code and client-safe body.
///
/// The single place this decision is made. On `master` it is duplicated across nine
/// `IntoResponse` impls and has drifted: `MenoError::NotFound` forwards the caller's
/// message while `Database` correctly does not, with no rule behind the difference.
#[must_use]
pub fn to_body(err: &Error) -> ErrorBody {
    let (status_code, message, data) = match err {
        Error::BadRequest { message, .. } => (400, message.clone(), None),
        Error::Unauthorized { message, .. } => (401, message.clone(), None),
        Error::Forbidden { message, .. } => (403, message.clone(), None),
        Error::NotFound { resource, .. } => (404, format!("{resource} not found"), None),
        Error::Conflict { message, .. } => (409, message.clone(), None),
        Error::Validation { fields } => (
            422,
            "One or more fields are invalid".to_string(),
            Some(fields.clone()),
        ),
        Error::RateLimited { retry_after_secs } => (
            429,
            format!("Too many requests. Retry in {retry_after_secs}s"),
            None,
        ),
        // 503: the caller may retry, so naming the dependency is useful — but the
        // upstream's own error text must not appear.
        Error::Upstream { service, .. } => {
            (503, format!("{service} is temporarily unavailable"), None)
        }
        // 500: generic. The detail is in the logs, correlated by request id.
        Error::Internal { .. } => (500, "An internal error occurred".to_string(), None),
    };

    ErrorBody {
        status_code,
        code: err.code().as_str(),
        message,
        status: false,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infrastructure_errors_never_leak_detail_to_the_client() {
        // The security property. A SQL error string contains table names, column names
        // and sometimes row values. None of it may reach the client.
        let err = Error::Internal {
            context: "load_broadcast",
            detail: "error returned from database: relation \"users_secret\" does not exist"
                .to_string(),
        };
        let body = to_body(&err);
        assert_eq!(body.status_code, 500);
        assert_eq!(body.code, "INTERNAL_ERROR");
        assert_eq!(body.message, "An internal error occurred");
        assert!(
            !body.message.contains("users_secret"),
            "schema names must not reach the client"
        );
        assert!(!err.is_client_safe());
    }

    #[test]
    fn upstream_detail_is_summarised_not_forwarded() {
        let err = Error::Upstream {
            service: "livekit",
            detail: "401 from https://livekit.internal:7880/admin".to_string(),
        };
        let body = to_body(&err);
        assert_eq!(body.status_code, 503);
        assert_eq!(body.code, "UPSTREAM_UNAVAILABLE");
        assert!(
            !body.message.contains("livekit.internal"),
            "upstream URLs must not leak"
        );
    }

    #[test]
    fn client_errors_keep_their_specific_code() {
        // The whole point of ErrorCode: clients branch on this, never on message text.
        let cases = [
            (
                Error::NotFound {
                    resource: "broadcast",
                    code: ErrorCode::NotFound,
                },
                404,
                "NOT_FOUND",
            ),
            (
                Error::Conflict {
                    code: ErrorCode::EmailTaken,
                    message: "taken".into(),
                },
                409,
                "EMAIL_TAKEN",
            ),
            (
                Error::BadRequest {
                    code: ErrorCode::StartTimeInPast,
                    message: "start_time must be in the future".into(),
                },
                400,
                "START_TIME_IN_PAST",
            ),
            (
                Error::RateLimited {
                    retry_after_secs: 30,
                },
                429,
                "RATE_LIMITED",
            ),
        ];
        for (err, status, code) in cases {
            let body = to_body(&err);
            assert_eq!(body.status_code, status);
            assert_eq!(body.code, code);
            assert!(!body.status, "every error body sets status:false");
        }
    }

    #[test]
    fn validation_details_are_structured_not_a_flat_string() {
        let mut fields = HashMap::new();
        fields.insert("email".to_string(), vec!["invalid format".to_string()]);
        let body = to_body(&Error::Validation { fields });
        assert_eq!(body.status_code, 422);
        let data = body.data.expect("validation carries per-field detail");
        assert_eq!(data["email"], vec!["invalid format".to_string()]);
    }

    #[test]
    fn every_code_maps_to_a_distinct_wire_string() {
        // Guards against copy-paste when adding codes — a duplicated string would
        // silently merge two error meanings in the client.
        let all = [
            ErrorCode::BadRequest,
            ErrorCode::InvalidCursor,
            ErrorCode::InvalidTimeZone,
            ErrorCode::StartTimeInPast,
            ErrorCode::Unauthorized,
            ErrorCode::InvalidCredentials,
            ErrorCode::InvalidToken,
            ErrorCode::TokenExpired,
            ErrorCode::RefreshTokenExpired,
            ErrorCode::Forbidden,
            ErrorCode::NotCreator,
            ErrorCode::NotOwner,
            ErrorCode::NotParticipant,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::EmailTaken,
            ErrorCode::AlreadyExists,
            ErrorCode::VersionConflict,
            ErrorCode::ValidationFailed,
            ErrorCode::RateLimited,
            ErrorCode::UpstreamUnavailable,
            ErrorCode::Internal,
        ];let mut seen = std::collections::HashSet::new();
        for code in all {
            assert!(seen.insert(code.as_str()), "duplicate wire string for {code:?}");
        }
        assert_eq!(seen.len(), all.len());
    }

    #[test]
    fn error_envelope_uses_snake_case_like_the_success_envelope() {
        // `MenoResponse` on master carries no `rename_all`, so it serialises
        // `status_code`. If this type ever gains `rename_all = "camelCase"`, errors and
        // successes stop agreeing on the field name and both clients break at once.
        let body = to_body(&Error::NotFound {
            resource: "broadcast",
            code: ErrorCode::NotFound,
        });
        let json = serde_json::to_string(&body).expect("serialises");
        assert!(
            json.contains("\"status_code\""),
            "clients parse status_code, got {json}"
        );
        assert!(
            !json.contains("statusCode"),
            "must not diverge from MenoResponse"
        );
    }
}
