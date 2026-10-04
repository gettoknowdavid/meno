//! One-time codes and password resets (plan §5.1, §9.3).
//!
//! # Why this is a second service and not more methods on `AuthService`
//!
//! §9.3 caps a file at ~400 lines, and §5.1's prescription for a service that has grown
//! past it is to split by *responsibility* rather than by line count. The line here is
//! real: the flows in this file all revolve around a six-digit code in an `otps` row,
//! share one code generator, one store-and-mail step and one spend step, and share the
//! enumeration-resistance rule. Nothing here signs a token or rotates a session — that is
//! [`super::services::AuthService`]'s job, and it borrows [`issue_response`] from there
//! for the one flow (verify-email) that has to mint a pair.
//!
//! # The two confidentiality rules this file enforces
//!
//! 1. **User enumeration.** [`CredentialService::resend_otp`] and
//!    [`CredentialService::forgot_password`] answer identically whether or not the
//!    address exists. An endpoint whose purpose is to send mail to an address the caller
//!    may not control cannot also be an oracle for "does this person have an account".
//! 2. **Best-effort mail.** [`CredentialService::send_code`] logs and swallows a send
//!    failure. Propagating it would tell a prober whether the account exists, and the
//!    code is still valid in the database — a retry of "resend" is the actual fix.

use std::time::Duration;

use time::OffsetDateTime;
use uuid::Uuid;

use super::dto::{
    AuthResponse, ForgotPasswordRequest, ResendOtpRequest, ResetPasswordRequest, Validatable,
    VerifyEmailRequest,
};
use super::error;
use super::mailer::{AuthEmail, AuthEmailRequest, AuthMailer};
use super::model::{Otp, OtpType, User};
use super::password;
use super::repository::AuthRepo;
use super::services::issue_response;
use super::token::TokenService;
use meno_core::Error as MenoError;

/// How long a one-time code is valid.
///
/// Ten minutes: long enough for a user who left the app to come back to it, short enough
/// that a code left in a mailbox — which is readable by whoever holds the mailbox — is
/// not useful for long. §3.2 notes the code is stored in plaintext while it lives, which
/// is the reason the window is not a day.
pub const OTP_TTL: Duration = Duration::from_secs(600);

/// Number of digits in a one-time code.
pub const OTP_DIGITS: usize = 6;

/// The one-time-code and password-reset flows.
///
/// Every dependency is an explicit trait object (§4.1), so a missing one is a compile
/// error rather than an `Option` unwrapped at runtime.
#[derive(Clone)]
pub struct CredentialService {
    repo: std::sync::Arc<dyn AuthRepo>,
    tokens: TokenService,
    mailer: std::sync::Arc<dyn AuthMailer>,
}

impl std::fmt::Debug for CredentialService {
    /// Nothing here prints a code or an address. `tokens` already redacts its keys.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CredentialService")
            .field("repo", &self.repo)
            .field("tokens", &self.tokens)
            .field("mailer", &self.mailer)
            .finish_non_exhaustive()
    }
}

/// Everything [`CredentialService::new`] needs.
#[derive(Debug, Clone)]
pub struct CredentialDeps {
    /// Storage.
    pub repo: std::sync::Arc<dyn AuthRepo>,
    /// Token issuing, for the pair a verified address gets.
    pub tokens: TokenService,
    /// Transactional mail.
    pub mailer: std::sync::Arc<dyn AuthMailer>,
}

impl CredentialService {
    /// Build the service.
    #[must_use]
    pub fn new(deps: CredentialDeps) -> Self {
        Self {
            repo: deps.repo,
            tokens: deps.tokens,
            mailer: deps.mailer,
        }
    }

    /// `POST /auth/verify-email`.
    ///
    /// # Errors
    ///
    /// [`error::invalid_otp`] for a wrong, spent, expired or wrong-kind code — one error
    /// for all four, so the endpoint is not an oracle.
    /// [`error::email_already_verified`] when the address was already confirmed, and
    /// whatever storage reports for the rest.
    #[tracing::instrument(skip_all, fields(user_email = %super::services::redact(&req.email)))]
    pub async fn verify_email(&self, req: &VerifyEmailRequest) -> Result<AuthResponse, MenoError> {
        req.validate()?;

        let email = User::normalized_email(&req.email);
        self.consume_code(&email, OtpType::VerifyEmail, &req.code)
            .await?;

        let user = self
            .repo
            .find_user_by_email(&email)
            .await?
            .ok_or(error::invalid_otp())?;

        if !self.repo.set_email_verified(user.id).await? {
            return Err(error::email_already_verified());
        }

        let mut verified = user;
        verified.verified = true;

        issue_response(&self.tokens, self.repo.as_ref(), verified).await
    }

    /// `POST /auth/resend-otp`.
    ///
    /// # Errors
    ///
    /// Currently nothing: the request succeeds whether or not the address exists,
    /// because the alternative is an enumeration oracle on an endpoint whose whole purpose
    /// is to send mail to an address the caller may not control. Validation and storage
    /// faults still propagate.
    #[tracing::instrument(skip_all, fields(user_email = %super::services::redact(&req.email)))]
    pub async fn resend_otp(&self, req: &ResendOtpRequest) -> Result<(), MenoError> {
        req.validate()?;

        let email = User::normalized_email(&req.email);
        if self.repo.find_user_by_email(&email).await?.is_none() {
            tracing::info!(
                kind = req.otp_type.as_str(),
                "resend for an unknown address"
            );
            return Ok(());
        }

        send_code(
            self.repo.as_ref(),
            self.mailer.as_ref(),
            &email,
            req.otp_type,
        )
        .await;
        Ok(())
    }

    /// `POST /auth/forgot-password`.
    ///
    /// # Errors
    ///
    /// Currently nothing, for the enumeration reason on [`Self::resend_otp`]. A session
    /// revocation failure is logged rather than propagated: telling the requester that
    /// sessions could not be ended would also tell them the account exists, and the
    /// reset itself still goes out.
    #[tracing::instrument(skip_all, fields(user_email = %super::services::redact(&req.email)))]
    pub async fn forgot_password(&self, req: &ForgotPasswordRequest) -> Result<(), MenoError> {
        req.validate()?;

        let email = User::normalized_email(&req.email);
        let Some(user) = self.repo.find_user_by_email(&email).await? else {
            tracing::info!("password reset requested for an unknown address");
            return Ok(());
        };

        send_code(
            self.repo.as_ref(),
            self.mailer.as_ref(),
            &email,
            OtpType::ResetPassword,
        )
        .await;

        // A reset request ends every session: the requester may not be the owner, and
        // leaving the old devices signed in would defeat the point of resetting.
        if let Err(problem) = self.tokens.revoke_all_for_user(user.id).await {
            tracing::error!(user_id = %user.id, error = %problem, "could not end sessions on reset");
        }

        Ok(())
    }

    /// `POST /auth/reset-password`.
    ///
    /// # Errors
    ///
    /// [`error::invalid_otp`] for an unusable code, [`MenoError::Upstream`] if the
    /// blocklist cannot be reached, and whatever storage reports for the rest.
    #[tracing::instrument(skip_all, fields(user_email = %super::services::redact(&req.email)))]
    pub async fn reset_password(&self, req: &ResetPasswordRequest) -> Result<(), MenoError> {
        req.validate()?;

        let email = User::normalized_email(&req.email);
        self.consume_code(&email, OtpType::ResetPassword, &req.code)
            .await?;

        let user = self
            .repo
            .find_user_by_email(&email)
            .await?
            .ok_or(error::invalid_otp())?;

        let hash = password::hash_password(&req.new_password)?;

        // The repository revokes every session in the same transaction, so a failure
        // here cannot leave a device signed in with the old password.
        self.repo.set_password_hash(user.id, &hash).await?;

        // The transaction cannot reach Redis, and without this the access tokens minted
        // before the reset keep working until they expire on their own.
        self.tokens.invalidate_access_tokens(user.id).await
    }

    /// Spend a code, refusing anything unusable with one error.
    ///
    /// # Errors
    ///
    /// [`error::invalid_otp`] for a wrong, spent, expired or wrong-kind code, and
    /// whatever storage reports.
    async fn consume_code(&self, email: &str, kind: OtpType, code: &str) -> Result<(), MenoError> {
        let now = OffsetDateTime::now_utc();
        if self
            .repo
            .consume_otp(email, kind, code, now)
            .await
            .map_err(|problem| {
                tracing::error!(error = %problem, "could not read a one-time code");
                problem
            })?
        {
            return Ok(());
        }

        Err(error::invalid_otp())
    }
}

/// Issue a code, store it, and mail it. Never fails.
///
/// Crate-visible rather than a method because registration needs it too, and
/// store-then-mail is one decision: a second caller that stored the code but skipped the
/// mail, or mailed before storing, would leave a user with a code that cannot be spent
/// and no way to learn why.
///
/// Both failure modes are logged and swallowed, for the enumeration reason in the module
/// docs. The code stays valid in the database either way, so "resend" is the fix.
pub(crate) async fn send_code(
    repo: &dyn AuthRepo,
    mailer: &dyn AuthMailer,
    email: &str,
    kind: OtpType,
) {
    let code = generate_code();
    let expires_at = OffsetDateTime::now_utc() + OTP_TTL;

    if let Err(problem) = repo.save_otp(email, kind, &code, expires_at).await {
        tracing::error!(error = %problem, kind = kind.as_str(), "could not store a one-time code");
        return;
    }

    let message = AuthEmailRequest {
        to: email.to_owned(),
        kind: match kind {
            OtpType::VerifyEmail => AuthEmail::VerifyEmail,
            OtpType::ResetPassword => AuthEmail::ResetPassword,
        },
        code,
    };

    if let Err(problem) = mailer.send(&message).await {
        tracing::error!(error = %problem, kind = kind.as_str(), "could not send a one-time code");
    }
}

/// A fresh numeric one-time code.
///
/// Drawn from the OS CSPRNG rather than from a counter or a clock: `rand::rng()` is
/// seeded from the OS and falls back to a per-thread source seeded by entropy, so a
/// predictable code is not reachable by choosing the host. §9.1 — no `unwrap`, and no
/// fallback to a constant, which would turn a six-digit space into a target.
fn generate_code() -> String {
    // rand 0.10 split the extension methods onto `RngExt`; `Rng` is only the core trait.
    use rand::RngExt;

    let mut rng = rand::rng();
    (0..OTP_DIGITS)
        .map(|_| char::from(b'0' + rng.random_range(0..10)))
        .collect()
}

/// A stored one-time code, for the repository contract and for tests.
#[must_use]
pub fn otp_row(email: &str, kind: OtpType, code: &str, expires_at: OffsetDateTime) -> Otp {
    Otp {
        id: Uuid::new_v4(),
        email: email.to_owned(),
        code: code.to_owned(),
        otp_type: kind.as_str().to_owned(),
        used: false,
        created_at: OffsetDateTime::now_utc(),
        expires_at,
    }
}
