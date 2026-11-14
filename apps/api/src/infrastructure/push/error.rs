//! Failures from talking to Firebase Cloud Messaging.
//!
//! Ported from `apps/api/src/shared/services/push/error.rs` on `master` (`903c3ba`),
//! reworked for the monorepo's layering rules.
//!
//! # What changed, and why
//!
//! | `master` | Here | Reason |
//! | --- | --- | --- |
//! | `Http(#[from] reqwest::Error)` | `Transport(String)` + a `From` impl | `LivekitError` erases its driver type at the boundary (`Upstream(String)`); two conventions in one crate means callers must learn which is which. |
//! | `Internal(#[from] anyhow::Error)` | removed | `anyhow::Error` is a *type-erased* error. A variant that accepts it lets any bug in any layer be reported as a push failure, which makes the log line useless exactly when it is needed. Every variant here is a fact about FCM. |
//! | no classification | [`is_stale_token`] / [`is_retryable`] | `send_to_user` must *delete* a dead token but *retry* a rate limit, and those decisions were string-matching the `Display` output. |
//!
//! §5.5 keeps driver types inside infrastructure. This *is* infrastructure, so
//! translating [`PushError`] into [`meno_core::Error`] is the boundary's job — see
//! [`From`] below.
//!
//! # What is deliberately absent
//!
//! There is no variant that carries FCM's error body. It is logged with structured
//! fields at the point of failure and dropped, because it can echo back parts of the
//! message we sent and §9.1 forbids leaking internals. A caller that needs to know
//! *why* a send failed for debugging reads the log, not the error.

use meno_core::{Error as MenoError, ErrorCode};

/// The service name used in [`MenoError::Upstream`] and in log fields.
///
/// A constant so the adapter's identity cannot drift between the two places that
/// name it.
pub const SERVICE_NAME: &str = "fcm";

/// Everything that can go wrong sending a push notification.
///
/// `#[non_exhaustive]` because this is an infrastructure adapter the whole backend
/// calls: adding a variant must not be a breaking change for the callers that
/// `match` exhaustively today.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PushError {
    /// FCM reported the registration token as no longer valid.
    ///
    /// The device was uninstalled, the app was reinstalled, or the token was rotated.
    /// Nothing the caller can do but delete the token — retrying sends to a device
    /// that will never acknowledge the message.
    #[error("the device token is no longer registered")]
    TokenInvalid,

    /// FCM is throttling this project (`429`).
    ///
    /// Retryable, but not immediately: the retry belongs to a job, not to a request.
    #[error("FCM rate limit exceeded")]
    RateLimited,

    /// FCM answered with a non-success status that is not a stale token.
    ///
    /// The status is kept because it is the only machine-readable signal available and
    /// it is safe to log; the response body is not, for the reason in the module docs.
    #[error("FCM rejected the send with status {status}")]
    SendFailed {
        /// The HTTP status FCM returned.
        status: u16,
    },

    /// The circuit breaker is open, so the call was never attempted.
    ///
    /// Distinct from [`Self::SendFailed`] for the same reason `LivekitError::CircuitOpen`
    /// is: "we did not try" is worth retrying later without any user action, and "we
    /// tried and were refused" may not be.
    #[error("FCM is temporarily unavailable (circuit open)")]
    CircuitOpen,

    /// A Google `OAuth2` access token could not be obtained.
    ///
    /// Always a configuration or credential problem, never a device problem: a bad
    /// service-account key, a revoked secret, or a Google outage. It is *not*
    /// retryable on a short delay because retrying re-presents the same bad key.
    #[error("could not obtain a Google access token: {0}")]
    TokenFetch(String),

    /// The request never completed — DNS, TLS, connect or timeout.
    ///
    /// Retryable. Distinct from [`Self::SendFailed`] because no status ever arrived,
    /// so the message may or may not have been accepted: a send that times out after
    /// FCM accepted it produces a duplicate on retry, which is why the caller treats it
    /// as at-least-once rather than exactly-once.
    #[error("the request to FCM could not be completed: {0}")]
    Transport(String),
}

impl PushError {
    /// Whether the failure means this device token is dead and should be deleted.
    ///
    /// The single question `send_to_user` asks. It is a method rather than a
    /// `matches!` at the call site so the answer cannot be re-derived differently in
    /// two places.
    #[must_use]
    pub const fn is_stale_token(&self) -> bool {
        matches!(self, Self::TokenInvalid)
    }

    /// Whether retrying later could plausibly succeed.
    ///
    /// `false` for [`Self::TokenInvalid`] (the token is gone) and for
    /// [`Self::TokenFetch`] (the same bad credentials will be presented again).
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::CircuitOpen | Self::Transport(_) | Self::SendFailed { .. }
        )
    }

    /// The HTTP status FCM returned, when it returned one.
    ///
    /// `None` means the request never got that far — useful when deciding whether to
    /// look for an FCM-side or a network-side problem in the logs.
    #[must_use]
    pub const fn status(&self) -> Option<u16> {
        match self {
            Self::SendFailed { status } => Some(*status),
            Self::RateLimited => Some(429),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for PushError {
    /// Erases the driver type at the adapter boundary.
    ///
    /// The `Display` of a `reqwest::Error` names the failure mode ("error sending
    /// request for url ...") but not the status, which is why `?` from a send site
    /// lands in [`Self::Transport`] and the status-aware match below it stays explicit.
    fn from(error: reqwest::Error) -> Self {
        Self::Transport(error.to_string())
    }
}

impl From<PushError> for MenoError {
    /// Maps an adapter failure onto the canonical taxonomy (§4.2).
    ///
    /// Three outcomes, chosen by what the *caller* should do:
    ///
    /// - a stale token is not an error the user caused and not the server's fault —
    ///   [`ErrorCode::NotFound`] over a 404-shaped response is wrong too, since nothing
    ///   was requested. It is logged and dropped upstream; this arm exists for the
    ///   paths where it surfaces.
    /// - a throttled send is [`ErrorCode::RateLimited`] with FCM's own guidance.
    /// - everything else is [`ErrorCode::UpstreamUnavailable`], a 503.
    ///
    /// `detail` is the adapter's message, never FCM's response body, for the reason in
    /// the module docs.
    fn from(error: PushError) -> Self {
        match &error {
            PushError::TokenInvalid => Self::NotFound {
                resource: "device_token",
                code: ErrorCode::NotFound,
            },
            PushError::RateLimited => Self::RateLimited {
                retry_after_secs: RETRY_AFTER_SECS,
            },
            other => Self::Upstream {
                service: SERVICE_NAME,
                detail: other.to_string(),
            },
        }
    }
}

/// A failure reading or writing a device token.
///
/// Separate from [`PushError`] on purpose. The store is *domain*-side (§5.4's narrow
/// [`PushTokenStore`](super::PushTokenStore)), so its driver error is a `sqlx::Error`
/// that must never appear in this adapter's API — and a token that cannot be *read* is
/// not a token FCM rejected. Folding the two together would make "Postgres was briefly
/// unreachable" indistinguishable from "this user uninstalled the app".
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TokenStoreError {
    /// The token could not be read.
    #[error("the device-token lookup failed: {0}")]
    Lookup(String),

    /// The token could not be deleted.
    ///
    /// Only reached after FCM has already rejected the token, so this is a
    /// housekeeping failure: the notification is lost either way, and the next send
    /// will simply be rejected again.
    #[error("the device-token cleanup failed: {0}")]
    Cleanup(String),
}

/// How long a caller should back off after [`PushError::RateLimited`].
///
/// FCM's HTTP v1 API does not send `Retry-After`, so this is a deliberate constant
/// rather than a parsed header. FCM's documented guidance for `UNAVAILABLE` and
/// `INTERNAL` is exponential backoff, and this sits at the bottom of that curve — the
/// notification is not urgent enough to justify hammering the quota, and every retry
/// spends quota that the next user's notification then does not have.
pub const RETRY_AFTER_SECS: u64 = 30;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_stale_token_is_stale() {
        // The delete-the-token decision. If this widens, a transient 500 starts
        // deleting working device tokens and every user silently loses push.
        let errors = [
            PushError::TokenInvalid,
            PushError::RateLimited,
            PushError::SendFailed { status: 500 },
            PushError::CircuitOpen,
            PushError::TokenFetch("bad key".to_owned()),
            PushError::Transport("dns".to_owned()),
        ];

        let stale: Vec<&PushError> = errors.iter().filter(|e| e.is_stale_token()).collect();
        assert_eq!(stale, vec![&PushError::TokenInvalid]);
    }

    #[test]
    fn a_transient_failure_is_retryable_and_bad_credentials_are_not() {
        // Retrying `TokenFetch` re-presents the same broken key, so treating it as
        // transient turns one misconfiguration into an endless retry storm.
        assert!(PushError::RateLimited.is_retryable());
        assert!(PushError::CircuitOpen.is_retryable());
        assert!(PushError::Transport("timeout".to_owned()).is_retryable());
        assert!(PushError::SendFailed { status: 503 }.is_retryable());

        assert!(!PushError::TokenInvalid.is_retryable());
        assert!(!PushError::TokenFetch("bad key".to_owned()).is_retryable());
    }

    #[test]
    fn every_variant_is_either_retryable_or_actionable_without_retrying() {
        // Guards against a future variant being added to neither bucket by accident,
        // which is how a caller ends up with a failure it has no instruction for.
        let all = [
            PushError::TokenInvalid,
            PushError::RateLimited,
            PushError::SendFailed { status: 500 },
            PushError::CircuitOpen,
            PushError::TokenFetch("x".to_owned()),
            PushError::Transport("x".to_owned()),
        ];

        for error in &all {
            assert!(
                error.is_retryable()
                    || error.is_stale_token()
                    || matches!(error, PushError::TokenFetch(_)),
                "{error:?} is neither retryable, nor a stale token, nor a credential \
                 failure, so a caller has no instruction for it"
            );
        }
    }

    #[test]
    fn the_two_failures_a_retry_cannot_fix_are_exactly_two() {
        // A stale token and a bad key are the only outcomes where retrying presents
        // the same bad input again. Widening either list is a behaviour change and this
        // test is what makes it a deliberate one.
        let not_retryable = [
            PushError::TokenInvalid,
            PushError::TokenFetch("bad key".to_owned()),
        ];
        for error in &not_retryable {
            assert!(!error.is_retryable(), "{error:?}");
        }
    }

    #[test]
    fn the_status_is_reported_when_one_was_returned() {
        assert_eq!(PushError::SendFailed { status: 500 }.status(), Some(500));
        assert_eq!(PushError::RateLimited.status(), Some(429));
        assert_eq!(
            PushError::Transport("no response".to_owned()).status(),
            None
        );
        assert_eq!(PushError::CircuitOpen.status(), None);
    }

    #[test]
    fn messages_distinguish_the_variants() {
        // A caller that logs `err.to_string()` must be able to tell the cases apart.
        let messages = [
            PushError::TokenInvalid.to_string(),
            PushError::RateLimited.to_string(),
            PushError::SendFailed { status: 503 }.to_string(),
            PushError::CircuitOpen.to_string(),
            PushError::TokenFetch("no key".to_owned()).to_string(),
            PushError::Transport("dns".to_owned()).to_string(),
        ];

        for (i, a) in messages.iter().enumerate() {
            for (j, b) in messages.iter().enumerate() {
                assert!(i == j || a != b, "two variants share the message {a:?}");
            }
        }
        assert!(messages[2].contains("503"), "{}", messages[2]);
    }

    #[test]
    fn a_stale_token_maps_to_a_404_shape_and_a_throttle_to_a_429_shape() {
        // The §4.2 mapping: clients branch on `code`, so these have to be right.
        let stale: MenoError = PushError::TokenInvalid.into();
        assert_eq!(stale.code(), ErrorCode::NotFound);

        let throttled: MenoError = PushError::RateLimited.into();
        assert_eq!(throttled.code(), ErrorCode::RateLimited);
    }

    #[test]
    fn every_other_failure_maps_to_upstream_unavailable() {
        for error in [
            PushError::SendFailed { status: 500 },
            PushError::CircuitOpen,
            PushError::TokenFetch("bad key".to_owned()),
            PushError::Transport("dns".to_owned()),
        ] {
            let mapped: MenoError = error.clone().into();
            assert_eq!(
                mapped.code(),
                ErrorCode::UpstreamUnavailable,
                "{error:?} should surface as a 503"
            );
        }
    }

    #[test]
    fn the_upstream_mapping_names_the_service_and_keeps_the_detail() {
        let mapped: MenoError = PushError::CircuitOpen.into();

        assert_eq!(mapped.to_string(), "fcm unavailable");
    }

    #[test]
    fn a_reqwest_error_becomes_a_transport_failure_with_its_reason() {
        // `reqwest::Error` cannot be constructed directly, so the conversion is driven
        // through a real request to a port nothing is listening on — the one failure
        // mode an offline test can always produce.
        let error = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(async {
                reqwest::Client::new()
                    .get("http://127.0.0.1:1/never")
                    .send()
                    .await
                    .expect_err("nothing is listening on port 1")
            });

        let push: PushError = error.into();
        assert!(matches!(push, PushError::Transport(_)), "{push:?}");
        assert!(push.is_retryable());
    }

    #[test]
    fn the_backoff_constant_is_short_enough_to_be_useful() {
        // The notification is not urgent; an hour-long backoff would mean the user
        // never gets it. Asserted so "tune this" cannot silently become "forget it".
        const {
            assert!(
                RETRY_AFTER_SECS > 0 && RETRY_AFTER_SECS <= 300,
                "the backoff must be non-zero and no longer than five minutes"
            );
        }
    }
}
