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
    /// The OAuth provider has not verified ownership of the email address.
    EmailNotVerified,
    /// The OAuth provider is switched off for this deployment.
    ProviderDisabled,
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
            Self::EmailNotVerified => "EMAIL_NOT_VERIFIED",
            Self::ProviderDisabled => "PROVIDER_DISABLED",
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
/// # Why the HTTP status is not in the body
///
/// The status already travels in the HTTP status line. Repeating it as a `statusCode`
/// body field is what Stripe, GitHub and Google AIP-193 all decline to do — and it is
/// the redundancy this type removes. [`ErrorBody::http_status`] exists so the transport
/// layer can set the status line; `#[serde(skip)]` keeps it off the wire.
///
/// # Why `camelCase`
///
/// Flutter (Dart) and Next.js (TypeScript) both use lowerCamelCase, and the WebSocket
/// layer on `master` is *already* camelCase — `ws/dto.rs` hand-writes `"userId"`,
/// `"broadcastId"` and `"gracePeriodInSecs"` into `serde_json::json!` literals. The
/// snake_case HTTP DTOs were the outlier, so this consolidates on the convention the
/// clients and the socket layer already assume.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    /// HTTP status for the response line. Never serialised.
    #[serde(skip)]
    pub http_status: u16,
    /// The stable wire code from [`ErrorCode::as_str`]. This is the field clients
    /// branch on — never `message`.
    pub code: &'static str,
    /// Human-readable message. Always generic for non-client-safe errors.
    pub message: String,
    /// Always `false`; mirrors the success envelope's `status` field.
    pub status: bool,
    /// Per-field detail. Present only for [`Error::Validation`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<HashMap<String, Vec<String>>>,
    /// Correlation metadata. Filled in by `apps/api`, which owns the request context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

/// Metadata common to both response envelopes.
///
/// Lives here rather than in a module of its own because `ErrorBody` is one half of the
/// envelope contract and `MenoResponse` is the other; `Meta` is the shared part. It is
/// pure serde data, so `crates/core` is the right home for it under the §2.1 purity
/// rule — `apps/api/src/types/meno_response.rs` imports it from here.
///
/// # Deliberately no `timestamp`
///
/// The client has its own clock, and the HTTP `Date` header already carries server time.
/// A body field would only add drift to debug. `request_id` earns its place because it
/// is the one value the server has and the client does not.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    /// Correlates this response with the server-side log lines that produced it.
    ///
    /// The guide requires 500s to be logged "correlated by request id", but nothing
    /// emits one into the response today — so a user reporting a failure gives you
    /// nothing to grep for. `tower-http`'s `request-id` middleware is already enabled
    /// in the workspace manifest; this is where its value surfaces.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,

    /// Build identifier, read from an env var at startup.
    ///
    /// Hardcoding this would make it silently lie, so it is omitted when unset rather
    /// than defaulted to something plausible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
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
        http_status: status_code,
        code: err.code().as_str(),
        message,
        status: false,
        data,
        meta: None,
    }
}

impl ErrorBody {
    /// Attach correlation metadata.
    ///
    /// [`to_body`] cannot populate this itself: `crates/core` has no request context by
    /// design, so only the transport layer in `apps/api` knows the request id. Keeping
    /// it out of `to_body` is what preserves that separation.
    #[must_use]
    pub fn with_meta(mut self, meta: Meta) -> Self {
        self.meta = Some(meta);
        self
    }
}

/// Build a validation failure from one field's messages.
///
/// The one-argument form, for the common case of a single rule failing.
#[must_use]
pub fn invalid_field(field: &str, message: impl Into<String>) -> Error {
    let mut fields: HashMap<String, Vec<String>> = HashMap::new();
    fields.insert(field.to_owned(), vec![message.into()]);
    Error::Validation { fields }
}

/// Build a validation failure from several fields at once.
///
/// Takes ownership so a caller can hand it a [`FieldErrors`] it built by accumulating,
/// which is how every `validate` method reports without cloning.
#[must_use]
pub fn invalid_fields(fields: HashMap<String, Vec<String>>) -> Error {
    Error::Validation { fields }
}

/// The accumulator [`invalid_fields`] is fed from.
///
/// A named type rather than a free function so the "create, fill, convert" shape is
/// visible in the signature of every `validate` method, and so the empty-map case is
/// handled once — a validation failure with no fields is a bug, and it is caught here
/// rather than shipped as `{code: "VALIDATION_FAILED", data: {}}`.
///
/// # Why it lives here and not in a module
///
/// It started in `modules/auth/error.rs`, which meant the second module to need one
/// would have had to copy it — and a copy that accumulates differently is how §4.2's
/// "duplicated nine times and drifted" happens again. A module may reach down to
/// `crates/core` and may not reach sideways into another module, so the accumulator
/// every module needs has to sit below both of them. It is pure `std` with no I/O, so
/// §2.1's purity rule is satisfied.
#[derive(Debug, Default)]
pub struct FieldErrors {
    fields: HashMap<String, Vec<String>>,
}

impl FieldErrors {
    /// An empty accumulator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `message` against `field`.
    pub fn push(&mut self, field: &str, message: impl Into<String>) {
        self.fields
            .entry(field.to_owned())
            .or_default()
            .push(message.into());
    }

    /// Record every message in `messages` against `field`.
    pub fn extend(&mut self, field: &str, messages: &[&str]) {
        for message in messages {
            self.push(field, *message);
        }
    }

    /// Whether anything failed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// The validation error, or `Ok(())` when nothing failed.
    ///
    /// # Errors
    ///
    /// [`Error::Validation`] carrying every accumulated field.
    pub fn into_result(self) -> Result<(), Error> {
        if self.fields.is_empty() {
            Ok(())
        } else {
            Err(invalid_fields(self.fields))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_accumulator_is_success_not_an_empty_failure() {
        // The bug this catches: a `validate` that returns
        // `Err(Validation { fields: {} })`, which renders as a 422 with no explanation.
        assert!(FieldErrors::new().into_result().is_ok());
    }

    #[test]
    fn validation_keeps_one_entry_per_field() {
        let mut fields = FieldErrors::new();
        fields.push("email", "An email address is required");
        fields.push("password", "A password must be at least 8 characters");

        let Error::Validation { fields } = fields.into_result().expect_err("failed") else {
            panic!("expected a validation failure");
        };

        assert_eq!(fields.len(), 2);
        assert_eq!(fields["email"], ["An email address is required"]);
        assert_eq!(
            fields["password"],
            ["A password must be at least 8 characters"]
        );
    }

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
        assert_eq!(body.http_status, 500);
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
        assert_eq!(body.http_status, 503);
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
            assert_eq!(body.http_status, status);
            assert_eq!(body.code, code);
            assert!(!body.status, "every error body sets status:false");
        }
    }

    #[test]
    fn validation_details_are_structured_not_a_flat_string() {
        let mut fields = HashMap::new();
        fields.insert("email".to_string(), vec!["invalid format".to_string()]);
        let body = to_body(&Error::Validation { fields });
        assert_eq!(body.http_status, 422);
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
        ];
        let mut seen = std::collections::HashSet::new();
        for code in all {
            assert!(
                seen.insert(code.as_str()),
                "duplicate wire string for {code:?}"
            );
        }
        assert_eq!(seen.len(), all.len());
    }

    #[test]
    fn the_wire_envelope_carries_only_the_documented_keys() {
        // This is the whole contract for both the error and success envelopes, and the
        // test is here so nobody quietly re-adds a redundant field.
        let body = to_body(&Error::NotFound {
            resource: "broadcast",
            code: ErrorCode::NotFound,
        });
        let json: serde_json::Value = serde_json::to_value(&body).expect("serialises");

        let keys: Vec<&String> = json.as_object().expect("object").keys().collect();
        assert_eq!(
            keys,
            vec!["code", "message", "status"],
            "unexpected envelope shape: {json}"
        );
        // The HTTP status belongs in the status line, not the body.
        assert!(
            !json.to_string().contains("http_status"),
            "the redundant status field must not reach the wire"
        );
        // Every key is already lowerCamelCase-safe; the attribute guarantees that stays
        // true when a multi-word field is added.
        for key in keys {
            assert!(
                !key.contains('_'),
                "{key} is snake_case; the wire is camelCase"
            );
        }
    }

    #[test]
    fn meta_is_camel_case_and_is_omitted_when_absent() {
        // Absent by default, so the common response stays lean.
        let plain = serde_json::to_value(to_body(&Error::RateLimited {
            retry_after_secs: 30,
        }))
        .expect("serialises");
        assert!(
            !plain.to_string().contains("meta"),
            "meta must be omitted, not sent as null: {plain}"
        );

        let with = serde_json::to_value(
            to_body(&Error::NotFound {
                resource: "broadcast",
                code: ErrorCode::NotFound,
            })
            .with_meta(Meta {
                request_id: Some("9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d".to_string()),
                api_version: Some("1.4.2".to_string()),
            }),
        )
        .expect("serialises");
        let meta = &with["meta"];
        assert_eq!(meta["requestId"], "9b1deb4d-3b7d-4bad-9bdd-2b0d7b3dcb6d");
        assert_eq!(meta["apiVersion"], "1.4.2");
        assert!(
            !meta.to_string().contains("request_id"),
            "meta keys are camelCase like the rest of the envelope"
        );

        // An unset api_version is dropped, not nulled - the client should be able to
        // treat "absent" as "this deployment did not report a version".
        let partial = serde_json::to_value(
            to_body(&Error::Internal {
                context: "ctx",
                detail: "d".to_string(),
            })
            .with_meta(Meta {
                request_id: Some("r".to_string()),
                api_version: None,
            }),
        )
        .expect("serialises");
        assert!(partial["meta"].get("apiVersion").is_none(), "{partial}");
    }

    #[test]
    fn validation_is_the_only_error_that_carries_data() {
        // Everything else omits `data` entirely via skip_serializing_if — never `null`,
        // which would force a null-check on a field that is conceptually optional.
        let mut fields = HashMap::new();
        fields.insert("email".to_string(), vec!["invalid format".to_string()]);
        let with =
            serde_json::to_value(to_body(&Error::Validation { fields })).expect("serialises");
        let keys: Vec<&String> = with.as_object().expect("object").keys().collect();
        assert_eq!(
            keys,
            vec!["code", "data", "message", "status"],
            "validation is the one envelope with a fourth key: {with}"
        );

        let without = serde_json::to_string(&to_body(&Error::RateLimited {
            retry_after_secs: 1,
        }))
        .expect("serialises");
        assert!(
            !without.contains("\"data\""),
            "skip_serializing_if must omit data entirely, not send null"
        );
    }
}
