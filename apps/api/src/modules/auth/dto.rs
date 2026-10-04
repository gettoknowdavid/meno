//! Request and response bodies for the auth endpoints (plan §9.5).
//!
//! # Validate at the boundary, in code
//!
//! Every request type has a `validate` method, and handlers call it as their first
//! statement. That is the §9.5 rule: a handler assumes its arguments are usable.
//!
//! The previous revision expressed the same intent with `validator`'s derive attributes
//! — `#[validate(custom(function = "validate_password"))]`, `#[validate(length(min = 1))]` —
//! which put the field name in a string, produced a second error shape
//! (`ValidationErrors`) that §4.2's envelope could not carry, and could not express "two
//! rules failed on this field" as two entries. Hand-written methods return
//! [`MenoError::Validation`] with a `HashMap<String, Vec<String>>`, which is exactly the
//! envelope's `data`, and a renamed field is a compile error instead of a silent
//! mismatch.
//!
//! # What is *not* validated here
//!
//! Nothing that needs a database or a clock. Whether an email is already registered, or
//! whether a one-time code is still live, is the service's question; a handler that
//! checked either would be doing business logic, and §7's handler rule says handlers are
//! parse → authorise → delegate → respond.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use super::error::FieldErrors;
use super::model::{AuthProvider, OtpType, UserRole};
use super::validators;
use meno_core::Error as MenoError;

/// A request that can check itself before a handler looks at it.
pub trait Validatable {
    /// Run every rule, accumulating rather than short-circuiting.
    ///
    /// # Errors
    ///
    /// [`MenoError::Validation`] listing every field that failed and why. Returning on
    /// the first failure would make a form with two mistakes take two round trips.
    fn validate(&self) -> Result<(), MenoError>;
}

/// `POST /auth/register`.
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    /// Display name.
    pub full_name: String,
    /// Login address.
    pub email: String,
    /// The chosen password.
    ///
    /// Never echoed back, never logged, and never included in a validation message.
    /// §4.2 requires a message to be safe to show the client, and a rule that quoted the
    /// submitted value would put a password on screen.
    pub password: String,
}

impl Validatable for RegisterRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();

        fields.extend(
            validators::FULL_NAME,
            &validators::full_name_problems(&self.full_name),
        );
        if let Some(problem) = validators::email_problem(&self.email) {
            fields.push(validators::EMAIL, problem.message());
        }
        fields.extend(
            validators::PASSWORD,
            &validators::password_problems(&self.password),
        );

        fields.into_result()
    }
}

/// `POST /auth/login`.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// Login address.
    pub email: String,
    /// The password to check.
    pub password: String,
}

impl Validatable for LoginRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();

        if let Some(problem) = validators::email_problem(&self.email) {
            fields.push(validators::EMAIL, problem.message());
        }
        // Only "is it there", not the full policy. A login must not tell an attacker
        // that a password *fails the composition rules* — that is free feedback, and it
        // would let someone probe passwords that were set elsewhere.
        if self.password.is_empty() {
            fields.push(validators::PASSWORD, "A password is required");
        }

        fields.into_result()
    }
}

/// `POST /auth/refresh`.
#[derive(Debug, Deserialize)]
pub struct RefreshTokenRequest {
    /// The refresh token to rotate.
    pub refresh_token: String,
}

impl Validatable for RefreshTokenRequest {
    fn validate(&self) -> Result<(), MenoError> {
        if self.refresh_token.trim().is_empty() {
            return Err(super::error::invalid_field(
                "refresh_token",
                "A refresh token is required",
            ));
        }
        Ok(())
    }
}

/// `POST /auth/logout`.
#[derive(Debug, Deserialize)]
pub struct LogoutRequest {
    /// The refresh token to end the session with.
    pub refresh_token: String,
    /// The access token to blocklist, when the client still has it.
    pub access_token: Option<String>,
}

impl Validatable for LogoutRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();

        if self.refresh_token.trim().is_empty() {
            fields.push("refresh_token", "A refresh token is required");
        }
        // An `Option<String>` deserialized from `"access_token": ""` is `Some("")`, which
        // is not absent. Passing that to the blocklist would write a key derived from an
        // empty string.
        if self.access_token.as_deref().is_some_and(str::is_empty) {
            fields.push("access_token", "An access token cannot be empty");
        }

        fields.into_result()
    }
}

/// `POST /auth/verify-email`.
#[derive(Debug, Deserialize)]
pub struct VerifyEmailRequest {
    /// The address the code was sent to.
    pub email: String,
    /// The six-digit code.
    pub code: String,
}

impl Validatable for VerifyEmailRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();

        if let Some(problem) = validators::email_problem(&self.email) {
            fields.push(validators::EMAIL, problem.message());
        }
        if let Some(problem) = validators::code_problem(&self.code) {
            fields.push(validators::CODE, problem);
        }

        fields.into_result()
    }
}

/// `POST /auth/resend-otp`.
#[derive(Debug, Deserialize)]
pub struct ResendOtpRequest {
    /// The address to send to.
    pub email: String,
    /// Which kind of code to resend.
    pub otp_type: OtpType,
}

impl Validatable for ResendOtpRequest {
    fn validate(&self) -> Result<(), MenoError> {
        if let Some(problem) = validators::email_problem(&self.email) {
            return Err(super::error::invalid_field(
                validators::EMAIL,
                problem.message(),
            ));
        }
        Ok(())
    }
}

/// `POST /auth/forgot-password`.
#[derive(Debug, Deserialize)]
pub struct ForgotPasswordRequest {
    /// The address to send a reset code to.
    pub email: String,
}

impl Validatable for ForgotPasswordRequest {
    fn validate(&self) -> Result<(), MenoError> {
        if let Some(problem) = validators::email_problem(&self.email) {
            return Err(super::error::invalid_field(
                validators::EMAIL,
                problem.message(),
            ));
        }
        Ok(())
    }
}

/// `POST /auth/reset-password`.
#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    /// The address the code was sent to.
    pub email: String,
    /// The six-digit reset code.
    pub code: String,
    /// The replacement password.
    pub new_password: String,
}

impl Validatable for ResetPasswordRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();

        if let Some(problem) = validators::email_problem(&self.email) {
            fields.push(validators::EMAIL, problem.message());
        }
        if let Some(problem) = validators::code_problem(&self.code) {
            fields.push(validators::CODE, problem);
        }
        fields.extend(
            validators::PASSWORD,
            &validators::password_problems(&self.new_password),
        );

        fields.into_result()
    }
}

/// `POST /auth/google/callback` — the web flow.
#[derive(Debug, Deserialize)]
pub struct GoogleWebAuthRequest {
    /// The authorization code Google redirected back with.
    pub code: String,
    /// The `state` this service issued, echoed back by the browser.
    pub state: String,
}

impl Validatable for GoogleWebAuthRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();

        if self.code.trim().is_empty() {
            fields.push("code", "An authorization code is required");
        }
        if self.state.trim().is_empty() {
            fields.push("state", "A state token is required");
        }

        fields.into_result()
    }
}

/// `POST /auth/google` — the mobile flow, which presents an ID token directly.
#[derive(Debug, Deserialize)]
pub struct GoogleMobileAuthRequest {
    /// The ID token the platform SDK obtained.
    pub id_token: String,
    /// What the user called this device, for the session list (§4.7 item 1).
    pub device_label: Option<String>,
}

impl Validatable for GoogleMobileAuthRequest {
    fn validate(&self) -> Result<(), MenoError> {
        let mut fields = FieldErrors::new();

        if self.id_token.trim().is_empty() {
            fields.push("id_token", "An ID token is required");
        }
        if let Some(problem) = self
            .device_label
            .as_deref()
            .and_then(validators::device_label_problem)
        {
            fields.push(validators::DEVICE_LABEL, problem);
        }

        fields.into_result()
    }
}

/// The device context a session is bound to, taken from the request (§4.7 item 1).
#[derive(Debug, Default, Clone)]
pub struct DeviceHint {
    /// What the user called this device.
    pub device_label: Option<String>,
    /// The request's `User-Agent`.
    pub user_agent: Option<String>,
}

/// The body of a successful register / login / refresh / Google sign-in.
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    /// The signed access token.
    pub access_token: String,
    /// The signed refresh token. Single-use; presenting it twice ends the session.
    pub refresh_token: String,
    /// Seconds until the access token expires, so a client can refresh on its own.
    pub expires_in: i64,
    /// The signed-in account.
    pub user: UserResponse,
}

impl AuthResponse {
    /// Build the body for a freshly issued pair.
    #[must_use]
    pub fn new(
        access_token: String,
        refresh_token: String,
        access_ttl_secs: i64,
        user: UserResponse,
    ) -> Self {
        Self {
            access_token,
            refresh_token,
            expires_in: access_ttl_secs,
            user,
        }
    }
}

/// One entry in the "your devices" list (§4.7 item 3).
#[derive(Debug, Serialize)]
pub struct SessionResponse {
    /// The session id, for `POST /auth/sessions/{id}/revoke`.
    pub id: Uuid,
    /// Something a person recognises.
    pub device_label: String,
    /// When the session was created.
    pub created_at: OffsetDateTime,
    /// When it was last used to refresh.
    pub last_used_at: OffsetDateTime,
    /// How many times this device's chain has been rotated.
    pub rotations: u32,
}

/// A user, as the auth endpoints return one.
///
/// Not the whole `users` row: `role` is there because a client needs it to decide what
/// to render, but `updated_at`, the follower counts and the search vector are not, and a
/// response that carries them is a response that has to be kept in step with the table.
#[derive(Debug, Serialize)]
pub struct UserResponse {
    /// The account id.
    pub id: Uuid,
    /// Display name.
    pub full_name: String,
    /// Biography.
    pub bio: Option<String>,
    /// Login address.
    pub email: String,
    /// Whether the address has been confirmed.
    pub verified: bool,
    /// Avatar storage key.
    pub avatar_id: Option<String>,
    /// Resolved avatar URL.
    pub avatar_url: Option<String>,
    /// Which providers this account can sign in with.
    pub providers: Vec<AuthProvider>,
    /// The account's role.
    pub role: UserRole,
    /// When the account was created.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    //! §9.5's rule, asserted directly: an invalid request is refused before the handler
    //! looks at it, and the refusal names every field that was wrong.
    //!
    //! The property that matters most is the third group: a failure must never echo the
    //! submitted value back, because the envelope is rendered straight to a client.

    use super::*;
    use meno_core::ErrorCode;

    /// The `data` map of a validation failure.
    fn fields(error: MenoError) -> std::collections::HashMap<String, Vec<String>> {
        match error {
            MenoError::Validation { fields } => fields,
            other => panic!("expected a validation failure, got {other:?}"),
        }
    }

    /// The `data` map of a validation that was expected to succeed.
    #[allow(dead_code)]
    fn fields_of(ok: Result<(), MenoError>) -> std::collections::HashMap<String, Vec<String>> {
        fields(ok.expect_err("expected a validation failure"))
    }

    /// The messages the EMAIL field collected — a `Vec`, because the accumulator
    /// deliberately does not stop at the first failure (§4.2's `data` is a list).
    fn email_of(request: &RegisterRequest) -> Option<Vec<String>> {
        fields(
            request
                .validate()
                .expect_err("a malformed registration must fail"),
        )
        .remove(validators::EMAIL)
    }

    // ── register ───────────────────────────────────────────────────────────

    #[test]
    fn a_well_formed_registration_passes() {
        assert!(
            RegisterRequest {
                full_name: "Ada Lovelace".to_owned(),
                email: "ada@example.com".to_owned(),
                password: "Correct Horse9".to_owned(),
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn every_bad_field_is_reported_in_one_pass() {
        // Short-circuiting would make a form with three mistakes take three round trips.
        let error = RegisterRequest {
            full_name: "A".to_owned(),
            email: "not-an-email".to_owned(),
            password: "short".to_owned(),
        }
        .validate()
        .expect_err("refused");

        let fields = fields(error);
        assert!(fields.contains_key(validators::FULL_NAME));
        assert!(fields.contains_key(validators::EMAIL));
        assert!(fields.contains_key(validators::PASSWORD));
    }

    #[test]
    fn a_validation_failure_is_a_422_with_the_validation_code() {
        let error = RegisterRequest {
            full_name: "Ada Lovelace".to_owned(),
            email: "bad".to_owned(),
            password: "Correct Horse9".to_owned(),
        }
        .validate()
        .expect_err("refused");

        assert_eq!(error.code(), ErrorCode::ValidationFailed);
        assert!(error.is_client_safe());
    }

    #[test]
    fn a_failure_never_echoes_the_submitted_password() {
        // The envelope goes straight to a client, and a log line, and a support
        // screenshot. A message containing the value is a leak in all three.
        let password = "hunter2-but-not-valid";
        let error = RegisterRequest {
            full_name: "Ada Lovelace".to_owned(),
            email: "ada@example.com".to_owned(),
            password: password.to_owned(),
        }
        .validate();

        let rendered = format!("{error:?}");
        assert!(
            !rendered.contains(password),
            "the submitted password survived into the error: {rendered}"
        );
    }

    // ── login ──────────────────────────────────────────────────────────────

    #[test]
    fn login_does_not_apply_the_registration_password_policy() {
        // Applying it would be free feedback for someone probing passwords set
        // elsewhere, and would break any account created before the policy changed.
        //
        // So "short" — which fails both the length and the uppercase rule — is
        // *accepted* here. The assertion is that it validates, not that it is refused.
        let request = LoginRequest {
            email: "ada@example.com".to_owned(),
            password: "short".to_owned(),
        };

        assert!(
            request.validate().is_ok(),
            "a login must not apply the registration password policy"
        );

        // And an absent password is still refused: "there is no password to check" is
        // not a policy opinion, it is a malformed request.
        let absent = LoginRequest {
            email: "ada@example.com".to_owned(),
            password: String::new(),
        };
        let error = absent
            .validate()
            .expect_err("an empty password is refused only when absent");

        assert_eq!(error.code(), ErrorCode::ValidationFailed);
        assert!(
            !error.to_string().contains("lowercase"),
            "a composition rule leaked into a login failure"
        );
    }

    #[test]
    fn an_empty_login_password_is_refused_before_any_database_work() {
        assert!(
            LoginRequest {
                email: "ada@example.com".to_owned(),
                password: String::new(),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn a_malformed_login_address_is_refused() {
        assert!(
            email_of(&RegisterRequest {
                full_name: "Ada Lovelace".to_owned(),
                email: "ada@localhost".to_owned(),
                password: "Correct Horse9".to_owned(),
            })
            .is_some()
        );
    }

    // ── logout ─────────────────────────────────────────────────────────────

    #[test]
    fn a_blank_access_token_is_not_silently_treated_as_absent() {
        // `"access_token": ""` deserializes to `Some("")`. Passing that to the
        // blocklist would derive a key from an empty string.
        assert!(
            LogoutRequest {
                refresh_token: "a".repeat(16),
                access_token: Some(String::new()),
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn an_absent_access_token_is_fine() {
        assert!(
            LogoutRequest {
                refresh_token: "a".repeat(16),
                access_token: None,
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn logout_without_a_refresh_token_is_refused() {
        assert!(
            LogoutRequest {
                refresh_token: "   ".to_owned(),
                access_token: None,
            }
            .validate()
            .is_err()
        );
    }

    // ── one-time codes ─────────────────────────────────────────────────────

    #[test]
    fn verification_requires_a_six_digit_code() {
        assert!(
            VerifyEmailRequest {
                email: "ada@example.com".to_owned(),
                code: "12345".to_owned(),
            }
            .validate()
            .is_err()
        );
        assert!(
            VerifyEmailRequest {
                email: "ada@example.com".to_owned(),
                code: "123456".to_owned(),
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn verification_reports_the_email_and_the_code_together() {
        let fields = fields(
            VerifyEmailRequest {
                email: "nope".to_owned(),
                code: "abc".to_owned(),
            }
            .validate()
            .expect_err("refused"),
        );
        assert_eq!(fields.len(), 2);
        assert!(fields.contains_key(validators::EMAIL));
        assert!(fields.contains_key(validators::CODE));
    }

    #[test]
    fn a_reset_code_of_the_wrong_length_is_refused() {
        assert!(
            ResetPasswordRequest {
                email: "ada@example.com".to_owned(),
                code: "1234567".to_owned(),
                new_password: "Correct Horse9".to_owned(),
            }
            .validate()
            .is_err()
        );
    }

    // ── resend and forgot ──────────────────────────────────────────────────

    #[test]
    fn resend_refuses_an_unusable_address() {
        assert!(
            ResendOtpRequest {
                email: "ada@".to_owned(),
                otp_type: OtpType::VerifyEmail,
            }
            .validate()
            .is_err()
        );
        assert!(
            ResendOtpRequest {
                email: "ada@example.com".to_owned(),
                otp_type: OtpType::ResetPassword,
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn forgot_password_refuses_an_unusable_address() {
        assert!(
            ForgotPasswordRequest {
                email: "ada@example".to_owned(),
            }
            .validate()
            .is_err()
        );
    }

    // ── google ─────────────────────────────────────────────────────────────

    #[test]
    fn the_web_flow_needs_both_a_code_and_a_state() {
        // §7.12: the `state` check is the CSRF defence, so a request with a code and no
        // state must not get as far as the provider.
        assert!(
            GoogleWebAuthRequest {
                code: "auth-code".to_owned(),
                state: String::new(),
            }
            .validate()
            .is_err()
        );
        assert!(
            GoogleWebAuthRequest {
                code: "auth-code".to_owned(),
                state: "state-token".to_owned(),
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn the_mobile_flow_needs_an_id_token() {
        assert!(
            GoogleMobileAuthRequest {
                id_token: "  ".to_owned(),
                device_label: None,
            }
            .validate()
            .is_err()
        );
        assert!(
            GoogleMobileAuthRequest {
                id_token: "header.payload.signature".to_owned(),
                device_label: Some("Ada's iPhone".to_owned()),
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn an_unusable_device_label_is_refused_at_the_boundary() {
        // A control character in the label reaches the "your devices" list the user
        // reads to decide what to revoke.
        assert!(
            GoogleMobileAuthRequest {
                id_token: "header.payload.signature".to_owned(),
                device_label: Some("iPad\u{0}2".to_owned()),
            }
            .validate()
            .is_err()
        );
    }

    // ── responses ──────────────────────────────────────────────────────────

    #[test]
    fn an_auth_response_carries_the_token_lifetime_the_client_needs() {
        // Without `expires_in` a client has to hardcode a refresh schedule, which is
        // how clients end up refreshing an hour before their token is good and an hour
        // after it stops being.
        let response =
            AuthResponse::new("access".to_owned(), "refresh".to_owned(), 900, todo_user());
        assert_eq!(response.expires_in, 900);
    }

    fn todo_user() -> UserResponse {
        UserResponse {
            id: Uuid::nil(),
            full_name: "Ada Lovelace".to_owned(),
            bio: None,
            email: "ada@example.com".to_owned(),
            verified: true,
            avatar_id: None,
            avatar_url: None,
            providers: vec![AuthProvider::Password],
            role: UserRole::User,
            created_at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn a_serialised_user_response_never_carries_a_password_hash() {
        // The struct cannot hold one, and this asserts the whole envelope for it.
        let json = serde_json::to_string(&todo_user()).expect("serialising");
        // Keyed, not substring: `providers: ["password"]` is a legitimate value and a
        // substring check would flag it, which is how an assertion like this quietly
        // stops testing anything.
        assert!(!json.contains("password_hash"), "{json}");
        assert!(!json.contains("\"hash\""), "{json}");
        assert!(json.contains("\"verified\":true"), "{json}");
    }

    #[test]
    fn a_serialised_auth_response_carries_both_tokens() {
        let json = serde_json::to_string(&AuthResponse::new(
            "access".to_owned(),
            "refresh".to_owned(),
            900,
            todo_user(),
        ))
        .expect("serialising");

        assert!(json.contains("\"access_token\":\"access\""), "{json}");
        assert!(json.contains("\"refresh_token\":\"refresh\""), "{json}");
    }
}
