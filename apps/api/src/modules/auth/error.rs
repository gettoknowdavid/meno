//! Auth's contribution to the single error taxonomy (plan §4.2).
//!
//! The previous revision of this file defined a twenty-variant `AuthError` enum with its
//! own `IntoResponse` impl. That is exactly what §4.2 names as the problem: *"nine
//! independent error enums … error mapping logic is duplicated nine times and drifts."*
//! Auth had a `ValidationError(#[from] ValidationErrors)` variant that meant something
//! different from every other module's validation error, and an `Internal(anyhow::Error)`
//! that put a driver-typed error one layer from the wire.
//!
//! So there is no enum here. There is one type — [`meno_core::Error`] — and this module
//! is the set of named ways auth constructs it. A function rather than a variant because
//! a constructor can *default* the parts that are always the same, which is where the
//! drift lived: `AuthError::Database(_) | AuthError::Redis(_) | AuthError::Internal(_)`
//! rendered as `500 INTERNAL_ERROR "An internal error occurred"`, and three different
//! call sites had to remember that pairing.
//!
//! # What a client can rely on
//!
//! The `code` is stable and machine-readable; the `message` is prose and may change.
//! Every function here picks a code from [`meno_core::ErrorCode`] and a message that
//! never contains driver output, a token, a password or an email address.
//!
//! # Why `not_found` for a user
//!
//! [`user_not_found`] exists but login never calls it. A login that distinguishes
//! "no such account" from "wrong password" is a user-enumeration oracle, so
//! [`invalid_credentials`] is returned for both — see
//! [`crate::modules::auth::services::AuthService::login`]. Keeping the constructor
//! available and unused is deliberate: the profile routes need it, and a reader who
//! finds it should find the comment explaining why login does not.

use std::collections::HashMap;

use meno_core::{Error as MenoError, ErrorCode};

/// The email is already registered to another account. §4.2 `Conflict`.
#[must_use]
pub fn email_taken() -> MenoError {
    MenoError::Conflict {
        code: ErrorCode::EmailTaken,
        message: "That email address is already registered".to_owned(),
    }
}

/// The email/password pair did not match an account.
///
/// The single answer to both "no such user" and "wrong password", so the response time
/// and the body are the same either way. §7.9 treats account enumeration as a real
/// finding and this is the mitigation.
#[must_use]
pub fn invalid_credentials() -> MenoError {
    MenoError::Unauthorized {
        code: ErrorCode::InvalidCredentials,
        message: "Email or password is incorrect".to_owned(),
    }
}

/// A token was structurally unacceptable: malformed, bad signature, or wrong algorithm.
#[must_use]
pub fn invalid_token() -> MenoError {
    MenoError::Unauthorized {
        code: ErrorCode::InvalidToken,
        message: "The token is not valid".to_owned(),
    }
}

/// An access token is past its expiry.
#[must_use]
pub fn access_token_expired() -> MenoError {
    MenoError::Unauthorized {
        code: ErrorCode::TokenExpired,
        message: "The access token has expired".to_owned(),
    }
}

/// A refresh token is past its expiry; the session has to be re-established.
#[must_use]
pub fn refresh_token_expired() -> MenoError {
    MenoError::Unauthorized {
        code: ErrorCode::RefreshTokenExpired,
        message: "The refresh token has expired".to_owned(),
    }
}

/// The account exists but the email address has not been verified.
///
/// §4.7 and the `VerifiedUser` extractor agree on the code, so a client that handles the
/// extractor's 403 handles this one identically.
#[must_use]
pub fn email_not_verified() -> MenoError {
    MenoError::Forbidden {
        code: ErrorCode::EmailNotVerified,
        message: "Verify your email address before continuing".to_owned(),
    }
}

/// The identity provider is switched off for this deployment (Google, §4.6).
#[must_use]
pub fn provider_disabled(provider: &'static str) -> MenoError {
    MenoError::Forbidden {
        code: ErrorCode::ProviderDisabled,
        message: format!("Signing in with {provider} is not enabled on this server"),
    }
}

/// An upstream authentication provider rejected or could not complete the exchange.
///
/// `detail` is logged and never returned: it is the provider's own text about the
/// user's account, which is not ours to echo.
#[must_use]
pub fn google_auth_failed(detail: impl std::fmt::Display) -> MenoError {
    MenoError::Upstream {
        service: "google",
        detail: detail.to_string(),
    }
}

/// A refresh token was presented whose session has already been rotated — §4.7 item 2.
///
/// The client is told the session is gone, not that it was reused. Announcing "we
/// detected theft" tells an attacker holding a stolen token that it *worked*, and a
/// legitimate user who simply raced their own phone is better served by being signed
/// out than by an explanation.
#[must_use]
pub fn refresh_token_reused() -> MenoError {
    MenoError::Unauthorized {
        code: ErrorCode::InvalidToken,
        message: "This session has ended. Sign in again".to_owned(),
    }
}

/// A session id does not exist, or is not the caller's.
///
/// One error for both, so that `POST /auth/sessions/{id}/revoke` cannot be used to probe
/// which session ids exist.
#[must_use]
pub fn session_not_found() -> MenoError {
    MenoError::NotFound {
        resource: "session",
        code: ErrorCode::NotFound,
    }
}

/// A user id has no matching live account.
#[must_use]
pub fn user_not_found() -> MenoError {
    MenoError::NotFound {
        resource: "user",
        code: ErrorCode::NotFound,
    }
}

/// A one-time code was wrong, already used, or expired.
///
/// One error for all three. Distinguishing "already used" from "wrong" tells an attacker
/// that a code they guessed was right.
#[must_use]
pub fn invalid_otp() -> MenoError {
    MenoError::BadRequest {
        code: ErrorCode::BadRequest,
        message: "That verification code is not valid".to_owned(),
    }
}

/// The email has already been verified.
#[must_use]
pub fn email_already_verified() -> MenoError {
    MenoError::Conflict {
        code: ErrorCode::Conflict,
        message: "That email address is already verified".to_owned(),
    }
}

/// The account has no password and must sign in with a provider instead.
#[must_use]
pub fn password_login_unavailable() -> MenoError {
    MenoError::Conflict {
        code: ErrorCode::Conflict,
        message: "This account signs in with a provider, not a password".to_owned(),
    }
}

/// A driver or adapter failure inside the auth module. Always `500`, always logged.
///
/// `context` is a `&'static str` naming the operation, which is what makes the log line
/// useful; `detail` is the driver's own text and never reaches the client.
#[must_use]
pub fn internal(context: &'static str, detail: impl std::fmt::Display) -> MenoError {
    MenoError::Internal {
        context,
        detail: detail.to_string(),
    }
}

/// Build a `§4.2` validation failure from one field's messages.
///
/// The one-argument form, for the common case of a single rule failing.
#[must_use]
pub fn invalid_field(field: &str, message: impl Into<String>) -> MenoError {
    let mut fields: HashMap<String, Vec<String>> = HashMap::new();
    fields.insert(field.to_owned(), vec![message.into()]);
    MenoError::Validation { fields }
}

/// Build a `§4.2` validation failure from several fields at once.
///
/// Takes ownership so the caller can hand it a `HashMap` it built with
/// [`push`], which is how `dto::validate` accumulates without cloning.
#[must_use]
pub fn invalid_fields(fields: HashMap<String, Vec<String>>) -> MenoError {
    MenoError::Validation { fields }
}

/// The accumulator [`invalid_fields`] is fed from.
///
/// A named type rather than a free function so the "create, fill, convert" shape is
/// visible in the signature of every `validate` method, and so the empty-map case is
/// handled once — a validation failure with no fields is a bug, and it is caught here
/// rather than shipped as `{code: "VALIDATION_FAILED", data: {}}`.
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

    /// The `§4.2` error, or `Ok(())` when nothing failed.
    ///
    /// # Errors
    ///
    /// [`MenoError::Validation`] carrying every accumulated field.
    pub fn into_result(self) -> Result<(), MenoError> {
        if self.fields.is_empty() {
            Ok(())
        } else {
            Err(invalid_fields(self.fields))
        }
    }
}

#[cfg(test)]
mod tests {
    //! Two properties, both of which §4.2 depends on.
    //!
    //! 1. Every constructor produces the code its callers promise, because a client
    //!    branches on the code and not on the message.
    //! 2. No constructor leaks anything. §4.2 requires infrastructure detail to be
    //!    logged and never returned, and a message that quoted a driver error would be
    //!    a leak wearing a 400.

    use super::*;
    use meno_core::to_body;

    fn body(error: &MenoError) -> meno_core::ErrorBody {
        to_body(error)
    }

    #[test]
    fn every_client_facing_code_is_the_one_the_taxonomy_defines() {
        // The mapping the whole API contract hangs on. If one of these drifts, a client
        // branch silently stops firing.
        assert_eq!(email_taken().code(), ErrorCode::EmailTaken);
        assert_eq!(invalid_credentials().code(), ErrorCode::InvalidCredentials);
        assert_eq!(invalid_token().code(), ErrorCode::InvalidToken);
        assert_eq!(access_token_expired().code(), ErrorCode::TokenExpired);
        assert_eq!(
            refresh_token_expired().code(),
            ErrorCode::RefreshTokenExpired
        );
        assert_eq!(email_not_verified().code(), ErrorCode::EmailNotVerified);
        assert_eq!(
            provider_disabled("Google").code(),
            ErrorCode::ProviderDisabled
        );
        assert_eq!(refresh_token_reused().code(), ErrorCode::InvalidToken);
        assert_eq!(user_not_found().code(), ErrorCode::NotFound);
        assert_eq!(session_not_found().code(), ErrorCode::NotFound);
        assert_eq!(invalid_otp().code(), ErrorCode::BadRequest);
    }

    #[test]
    fn an_authenticated_but_unverified_caller_is_forbidden_not_unauthorized() {
        // 403, not 401: the token is fine, the account's state is not. §4.7's
        // `VerifiedUser` extractor returns the same code for the same reason.
        assert_eq!(to_body(&email_not_verified()).http_status, 403);
        assert_eq!(to_body(&invalid_credentials()).http_status, 401);
    }

    #[test]
    fn a_reused_refresh_token_reports_the_same_code_as_any_other_bad_token() {
        // Deliberate. Distinguishing "this was stolen" from "this was garbage" tells
        // whoever is holding it that it once worked.
        assert_eq!(
            refresh_token_reused().code(),
            invalid_token().code(),
            "reuse detection must not announce itself on the wire"
        );
    }

    #[test]
    fn infrastructure_detail_is_logged_never_returned() {
        let error = internal("load_user", "connection to 10.0.0.4:5432 refused");

        assert_eq!(to_body(&error).http_status, 500);
        assert!(
            !error.is_client_safe(),
            "an internal failure is never client-safe"
        );

        let rendered = to_body(&error);
        assert!(
            !rendered.message.contains("10.0.0.4"),
            "the address leaked into the rendered body: {}",
            rendered.message
        );
        // …but it is still on the error, for the log line.
        assert!(error.to_string().contains("load_user"));
    }

    #[test]
    fn a_provider_failure_does_not_echo_the_provider() {
        // `detail` is Google's text about the user's account.
        let error = google_auth_failed("access_denied: user cancelled");
        assert_eq!(to_body(&error).http_status, 503);
        assert!(
            !to_body(&error).message.contains("access_denied"),
            "provider text leaked to the client: {}",
            to_body(&error).message
        );
    }

    #[test]
    fn validation_keeps_one_entry_per_field() {
        let mut fields = FieldErrors::new();
        fields.push("email", "An email address is required");
        fields.push("password", "A password must be at least 8 characters");

        let MenoError::Validation { fields } = fields.into_result().unwrap_err() else {
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
    fn several_problems_on_one_field_stay_separate_entries() {
        // Not joined into one sentence. A Flutter form highlights a field; it cannot
        // half-highlight it because two rules failed.
        let mut fields = FieldErrors::new();
        fields.extend("password", &["too short", "needs a digit"]);

        let MenoError::Validation { fields } = fields.into_result().unwrap_err() else {
            panic!("expected a validation failure");
        };
        assert_eq!(fields["password"].len(), 2);
    }

    #[test]
    fn an_empty_accumulator_is_success_not_an_empty_failure() {
        // The bug this catches: a `validate` that returns
        // `Err(Validation { fields: {} })`, which renders as a 422 with no explanation.
        assert!(FieldErrors::new().into_result().is_ok());
    }

    #[test]
    fn a_repeated_message_on_one_field_is_kept() {
        // Not deduplicated: `validator` appended, and a client that shows the first
        // message must not be handed a duplicate. Two rules genuinely firing twice is
        // not a case this module can produce, and de-duplicating would hide one.
        let mut fields = FieldErrors::new();
        fields.push("email", "same");
        fields.push("email", "same");
        let MenoError::Validation { fields } = fields.into_result().unwrap_err() else {
            panic!("expected a validation failure");
        };
        assert_eq!(fields["email"].len(), 2);
    }

    #[test]
    fn an_error_body_never_carries_the_detail_field() {
        // `ErrorBody::data` is for validation field maps only. Confirming that an
        // internal failure does not smuggle its detail through `data`.
        let rendered = to_body(&internal("hash_password", "argon2: out of memory"));
        assert!(rendered.data.is_none(), "internal detail reached `data`");
    }

    #[test]
    fn session_and_user_not_found_are_the_same_shape() {
        // `POST /auth/sessions/{id}/revoke` uses `session_not_found` for both "no such
        // session" and "not yours"; `user_not_found` exists for the profile routes. Both
        // must render identically apart from the resource word.
        assert_eq!(body(&session_not_found()).http_status, 404);
        assert_eq!(body(&user_not_found()).http_status, 404);
        assert_eq!(
            body(&session_not_found()).code,
            ErrorCode::NotFound.as_str()
        );
        assert_eq!(body(&user_not_found()).code, ErrorCode::NotFound.as_str());
    }
}
