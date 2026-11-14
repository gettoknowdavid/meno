//! Failures from talking to an OAuth/OIDC provider.
//!
//! # Why this is a second error type, not a second taxonomy
//!
//! §4.2 replaced nine parallel error enums with one `crates/core::Error`. That is done,
//! and this type does not reopen it: [`OAuthError`] is **adapter-local**, living inside
//! `infrastructure/oauth` and converting into [`MenoError`] at the boundary — the same
//! arrangement as [`crate::infrastructure::push::PushError`] and
//! [`crate::infrastructure::storage::StorageError`].
//!
//! §5.5 is the reason. An adapter is the one layer allowed to name `oauth2::Error` and
//! `reqwest::Error`, because that is where they are logged and erased. Deleting
//! `OAuthError` and making `exchange_code` return `meno_core::Error` directly would move
//! driver types into the type that reaches the client, which is the leak §5.5 describes.
//!
//! What the single taxonomy guarantees — and what the tests here pin — is that no caller
//! ever sees an `OAuthError`, and no driver type ever escapes this module.

use meno_core::{Error as MenoError, ErrorCode};

/// The service name used in [`MenoError::Upstream`] and in log fields.
pub const SERVICE_NAME: &str = "google-oauth";

/// Everything that can go wrong during a Google sign-in.
///
/// `#[non_exhaustive]` because callers `match` on it to decide whether to offer a
/// fallback, and adding a variant must not break them.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum OAuthError {
    /// The client could not be built from configuration.
    ///
    /// A malformed `GOOGLE_REDIRECT_URI` is the realistic case. `master` called
    /// `.expect("Invalid Google redirect URI")` here, which turned a typo in one
    /// variable into a panic trace at boot (§9.1).
    #[error("OAuth is misconfigured: {0}")]
    Config(String),

    /// The provider refused to exchange the authorization code.
    ///
    /// Usually an expired code, a reused code, or a redirect-URI mismatch. All three are
    /// client-triggered and non-retryable.
    #[error("the authorization code was rejected: {0}")]
    CodeRejected(String),

    /// The provider was unreachable or returned a non-success status.
    ///
    /// Distinct from [`Self::CodeRejected`] for the same reason `LivekitError::CircuitOpen`
    /// is: "we did not try" is worth retrying later, "we tried and were refused" is not.
    #[error("the OAuth provider failed: {0}")]
    Upstream(String),

    /// The provider answered, but the identity is not one we will accept.
    ///
    /// [`Self::EmailNotVerified`] is the security-relevant one — see §7.12.
    #[error("the provider's identity was rejected: {0}")]
    Rejected(String),

    /// The provider's own address is unverified, so it cannot be linked to an account.
    ///
    /// §7.12: linking by email alone lets an attacker holding an *unverified* Google
    /// account with a victim's address take over that account. This variant exists so the
    /// refusal is a distinct, testable decision rather than a `bool` buried in a
    /// `verify_id_token` helper that one of the two call sites forgot to check.
    #[error("the provider has not verified this email address")]
    EmailNotVerified,

    /// Google sign-in is disabled, and the operation needed it.
    #[error("Google sign-in is disabled")]
    Disabled,
}

impl OAuthError {
    /// Whether retrying the same operation could plausibly succeed.
    ///
    /// Only an upstream fault qualifies. A rejected code is spent, and an unverified
    /// email does not become verified because we asked again.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Upstream(_))
    }

    /// The stable wire code for this error.
    ///
    /// [`Self::Disabled`] is `Forbidden` rather than `UpstreamUnavailable`: the client
    /// asking to sign in with Google on a deployment that has it switched off is not a
    /// server fault, and a 403 tells it to offer the password form instead of retrying.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::CodeRejected(_) | Self::Rejected(_) | Self::EmailNotVerified => {
                ErrorCode::BadRequest
            }
            Self::Disabled => ErrorCode::Forbidden,
            Self::Config(_) | Self::Upstream(_) => ErrorCode::UpstreamUnavailable,
        }
    }
}

impl From<reqwest::Error> for OAuthError {
    /// Erase the driver error; keep only whether it is worth retrying.
    fn from(error: reqwest::Error) -> Self {
        Self::Upstream(error.to_string())
    }
}

impl From<OAuthError> for MenoError {
    /// The boundary translation — `reqwest` and `oauth2` stop here (§5.5).
    ///
    /// [`MenoError::BadRequest`] for everything the client can act on, and
    /// [`MenoError::Upstream`] for a provider fault, which `to_body` renders as a bare
    /// `503 google-oauth is temporarily unavailable` — no endpoint, no client id, no
    /// provider error text (§9.1).
    fn from(error: OAuthError) -> Self {
        match error {
            OAuthError::CodeRejected(message) | OAuthError::Rejected(message) => Self::BadRequest {
                code: ErrorCode::BadRequest,
                message,
            },
            // The provider said no, so the client is told what to do instead: do not
            // retry, and do not offer to link this address. Named codes rather than the
            // generic `Forbidden` so a client can distinguish "this email is not verified"
            // from "you may not do that".
            OAuthError::EmailNotVerified => Self::Forbidden {
                code: ErrorCode::EmailNotVerified,
                message: "the provider has not verified this email address".to_owned(),
            },
            OAuthError::Disabled => Self::Forbidden {
                code: ErrorCode::ProviderDisabled,
                message: "this sign-in method is not available".to_owned(),
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
    fn an_unverified_email_is_forbidden_not_a_server_fault() {
        // §7.12. This is the whole point of the variant: the refusal must be a decision
        // the caller cannot overlook, and it must not read as "Google is having a bad
        // day, try again".
        let error = OAuthError::EmailNotVerified;

        assert_eq!(error.code(), ErrorCode::BadRequest);
        assert!(
            !error.is_retryable(),
            "an unverified email cannot be retried"
        );

        let meno: MenoError = error.into();
        assert_eq!(meno.code(), ErrorCode::EmailNotVerified);
        // Client-safe is correct here: the client *should* see this one, so it can show
        // "use a different Google account" rather than retrying. What it must never see is
        // a provider detail, which `to_body` is asserted on separately below.
        assert!(meno.is_client_safe());
    }

    #[test]
    fn a_rejected_code_is_the_clients_fault_and_not_retryable() {
        let error = OAuthError::CodeRejected("redirect_uri mismatch".to_owned());

        assert!(!error.is_retryable());

        let meno: MenoError = error.into();
        assert_eq!(meno.code(), ErrorCode::BadRequest);
        assert!(meno.is_client_safe());
    }

    #[test]
    fn an_upstream_fault_is_retryable_and_never_client_safe() {
        let error = OAuthError::Upstream("connection reset".to_owned());

        assert!(error.is_retryable());

        let meno: MenoError = error.into();
        assert_eq!(meno.code(), ErrorCode::UpstreamUnavailable);
        assert!(!meno.is_client_safe());
    }

    #[test]
    fn a_disabled_provider_is_forbidden_so_the_client_offers_a_password_form() {
        let error = OAuthError::Disabled;
        assert!(!error.is_retryable());

        let meno: MenoError = error.into();
        assert_eq!(meno.code(), ErrorCode::ProviderDisabled);
    }

    #[test]
    fn the_provider_detail_never_reaches_the_rendered_response() {
        // §9.1. `Debug` legitimately carries the detail — that is where it belongs, for
        // the log. `to_body` is what `IntoResponse` renders, and it must not.
        let meno: MenoError =
            OAuthError::Upstream("https://accounts.google.com/o/oauth2/v2/auth".to_owned()).into();

        let body = meno_core::to_body(&meno);

        assert_eq!(body.http_status, 503);
        assert_eq!(body.code, ErrorCode::UpstreamUnavailable.as_str());
        assert!(
            !body.message.contains("accounts.google.com"),
            "the provider endpoint must not survive into the response: {}",
            body.message
        );
        assert!(!format!("{body:?}").contains("accounts.google.com"));
    }

    #[test]
    fn the_service_name_is_stable() {
        // If this drifts, log lines and `Error::Upstream.service` disagree and a Grafana
        // search misses half the events.
        assert_eq!(SERVICE_NAME, "google-oauth");
    }
}
