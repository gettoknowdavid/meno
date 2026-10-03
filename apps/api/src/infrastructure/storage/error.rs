//! Failures from talking to the object store.
//!
//! # Why this exists rather than `anyhow::Result`
//!
//! `master`'s `storage.rs` returned `anyhow::Result` from every method. That is the
//! §4.2 problem in miniature: `anyhow::Error` is *type-erased*, so a variant that accepts
//! it lets any bug in any layer be reported as a storage failure, which makes the log
//! line useless exactly when it is needed. Every variant here is a fact about the object
//! store.
//!
//! # What changed from `master`
//!
//! | `master` | Here | Reason |
//! | --- | --- | --- |
//! | `anyhow::Result` everywhere | [`StorageError`] | §4.2 — one error taxonomy, domain detail in variants. |
//! | `.expect("Failed to build storage client")` in the constructor | [`StorageError::Config`] | §9.1 — no `expect` outside tests. A bad endpoint is a startup *error*, not a panic trace. |
//! | `object_exists` folded the driver error into `anyhow` | [`StorageError::NotFound`] | A missing object is a normal answer, not a failure. Callers were pattern-matching the `Display` string. |
//! | no key validation | [`StorageError::InvalidKey`] | §9.5 — a key from user input can contain `..` and escape the prefix. |
//!
//! §5.5 keeps driver types inside infrastructure. This *is* infrastructure, so holding an
//! `object_store::Error` here is correct; translating it into [`meno_core::Error`] is the
//! boundary's job — see [`From`] below.

use object_store::path::Error as PathError;

use meno_core::{Error as MenoError, ErrorCode};

/// The service name used in [`MenoError::Upstream`] and in log fields.
///
/// A constant so the adapter's identity cannot drift between the two places that name
/// it.
pub const SERVICE_NAME: &str = "storage";

/// Everything that can go wrong using object storage.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum StorageError {
    /// The store could not be built from the supplied configuration.
    ///
    /// Reported rather than `expect`ed (§9.1). In practice this means a malformed
    /// `STORAGE_ENDPOINT` or a missing bucket name: a startup failure with a readable
    /// message beats a panic trace on a deploy.
    #[error("object storage is misconfigured: {0}")]
    Config(String),

    /// An object key is unusable.
    ///
    /// §9.5 requires validating input at the boundary, and the boundary here is the key:
    /// a key of `../../etc/passwd` or an empty string is a client error, not a store
    /// failure. Rejecting it before it reaches the driver is also what keeps one user's
    /// upload from overwriting another's prefix.
    #[error("invalid object key: {0}")]
    InvalidKey(String),

    /// The requested object does not exist.
    ///
    /// A variant rather than a `bool` return so `object_exists` and `delete` can be
    /// honest about *why* they failed: `master` mapped `NotFound` to `Ok(false)` in one
    /// method and `Ok(())` in the other and lost the distinction everywhere else.
    #[error("no object at `{0}`")]
    NotFound(String),

    /// The store refused the operation.
    ///
    /// The upstream's own text is kept for the log and erased at the boundary — §9.1
    /// forbids leaking internals, and an S3 error body can echo back the endpoint and
    /// bucket name.
    #[error("object storage request failed: {0}")]
    Upstream(String),

    /// Storage is disabled, and the operation needed it.
    ///
    /// Distinct from [`Self::Upstream`] because it is a *configuration* state, not a
    /// fault: retrying will not help, but restarting with `STORAGE_ENABLED=true` will.
    #[error("object storage is disabled")]
    Disabled,
}

impl StorageError {
    /// Whether retrying the same operation could plausibly succeed.
    ///
    /// A misconfigured or disabled store will fail identically forever; a missing object
    /// only resolves if something uploads it first.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Upstream(_))
    }

    /// The stable wire code for this error.
    ///
    /// Every failure is `UPSTREAM_UNAVAILABLE` except the client-attributable ones, which
    /// are `BAD_REQUEST` — a client that sends `../` should be told to fix its request,
    /// not that storage is having a bad day.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::InvalidKey(_) => ErrorCode::BadRequest,
            Self::Config(_) | Self::NotFound(_) | Self::Upstream(_) | Self::Disabled => {
                ErrorCode::UpstreamUnavailable
            }
        }
    }
}

impl From<object_store::Error> for StorageError {
    /// Erase the driver error, keeping only the classification callers act on.
    ///
    /// `NotFound` becomes [`StorageError::NotFound`] because that is the one case a
    /// caller must branch on; everything else becomes [`StorageError::Upstream`] with the
    /// driver's text retained for the log line. §9.1 permits the detail in a log and
    /// forbids it in a response, and [`From`] below is where that separation is enforced.
    fn from(error: object_store::Error) -> Self {
        match error {
            object_store::Error::NotFound { path, .. } => Self::NotFound(path),
            other => Self::Upstream(other.to_string()),
        }
    }
}

impl From<PathError> for StorageError {
    fn from(error: PathError) -> Self {
        Self::InvalidKey(error.to_string())
    }
}

impl From<StorageError> for MenoError {
    /// The boundary translation: `object_store` stops here (§5.5).
    ///
    /// An [`MenoError::BadRequest`] for [`StorageError::InvalidKey`] because the client
    /// can fix it; [`MenoError::Upstream`] with [`SERVICE_NAME`] for everything else,
    /// which `IntoResponse` renders as a generic 503 so no endpoint, bucket name or
    /// driver message reaches the client.
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::InvalidKey(message) => Self::BadRequest {
                code: ErrorCode::BadRequest,
                message,
            },
            other => Self::Upstream {
                service: SERVICE_NAME,
                detail: other.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the classification and the boundary translation.

    use super::*;

    #[test]
    fn an_invalid_key_is_a_client_error_not_an_outage() {
        // The whole point of the variant: a client sending `../` must be told to fix its
        // request. Mapping it to `Upstream` would produce a 503 for a 400.
        let error = StorageError::InvalidKey("key must not contain `..`".to_owned());

        assert_eq!(error.code(), ErrorCode::BadRequest);
        assert!(!error.is_retryable());

        let meno: MenoError = error.into();
        assert_eq!(meno.code(), ErrorCode::BadRequest);
        assert!(meno.is_client_safe());
    }

    #[test]
    fn a_missing_object_is_not_retryable() {
        // Retrying a `head` on a missing object only succeeds if something uploads in
        // between, which is not something the storage adapter can arrange.
        assert!(!StorageError::NotFound("avatars/1/a.png".to_owned()).is_retryable());
    }

    #[test]
    fn an_upstream_failure_is_retryable_and_never_client_safe() {
        let error = StorageError::Upstream("connection reset".to_owned());

        assert!(error.is_retryable());

        let meno: MenoError = error.into();
        assert_eq!(meno.code(), ErrorCode::UpstreamUnavailable);
        assert!(
            !meno.is_client_safe(),
            "an upstream detail must never be shown to the client"
        );
    }

    #[test]
    fn a_disabled_store_is_upstream_not_a_panic() {
        let error = StorageError::Disabled;

        assert_eq!(error.code(), ErrorCode::UpstreamUnavailable);
        assert!(!error.is_retryable());
        assert_eq!(error.to_string(), "object storage is disabled");
    }

    #[test]
    fn the_service_name_is_the_one_the_error_taxonomy_uses() {
        // If this drifts, log lines and `Error::Upstream.service` disagree and a search
        // across Grafana misses half the events.
        assert_eq!(SERVICE_NAME, "storage");
    }

    #[test]
    fn the_upstream_detail_never_reaches_the_rendered_response() {
        // §9.1. `Debug` legitimately carries the detail — that is where it belongs, for
        // the log. What must not happen is the detail surviving into the body the client
        // receives, so this asserts on `to_body`, which is what `IntoResponse` renders.
        let meno: MenoError =
            StorageError::Upstream("https://acct.r2.cloudflarestorage.com".to_owned()).into();

        let body = meno_core::to_body(&meno);

        assert_eq!(body.http_status, 503);
        assert_eq!(body.code, ErrorCode::UpstreamUnavailable.as_str());
        assert!(
            !body.message.contains("cloudflarestorage.com"),
            "the upstream endpoint must not survive into the response: {}",
            body.message
        );
        assert!(!format!("{body:?}").contains("cloudflarestorage.com"));
    }
}
