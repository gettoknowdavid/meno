//! Auth orchestration: the layer between HTTP and storage (plan §4.1, §4.8).
//!
//! # What belongs here
//!
//! Ordering, and only ordering. Every decision about *what* is acceptable was made by
//! [`super::dto`] (shape) or [`super::validators`] (rules); every question about *what
//! exists* is the repository's. What is left — "hash before comparing", "do not reveal
//! whether the account exists", "rotate before minting" — is the part that has to be
//! right in a specific order, and it has no other home.
//!
//! The one-time-code and password-reset flows are in [`super::credentials`]. They are a
//! separate service rather than more methods here because they share a code generator, a
//! store-and-mail step and an enumeration rule, and none of which needs a session
//! (§5.1's split-by-responsibility, §9.3's line cap).
//!
//! # §4.8 observability
//!
//! Every public method is `#[tracing::instrument]` with structured fields and no
//! `format!` in the log message. The fields use `skip` for anything sensitive: an email
//! address, a password, a token and a one-time code never appear in a log line.
//!
//! # User enumeration
//!
//! [`AuthService::login`] answers identically whether or not the address exists, and
//! still verifies a password when no account matched — against the dummy hash built at
//! startup — so the timing does not give it away either.

use std::sync::Arc;
use std::time::Instant;

use uuid::Uuid;

use super::dto::{
    AuthResponse, DeviceHint, GoogleMobileAuthRequest, GoogleWebAuthRequest, LoginRequest,
    LogoutRequest, RefreshTokenRequest, RegisterRequest, SessionResponse, UserResponse,
    Validatable,
};
use super::error;
use super::google::{GoogleAuthorize, GoogleExchange, GoogleIdentity};
use super::mailer::AuthMailer;
use super::model::{AuthProvider, DeviceContext, NewUser, OtpType, User};
use super::password;
use super::repository::AuthRepo;
use super::token::TokenService;
use meno_core::Error as MenoError;

/// Auth's sign-in, registration and session use cases.
///
/// Every dependency is an explicit trait object (§4.1) and every one is required, so a
/// missing dependency is a compile error rather than an `Option` unwrapped at runtime.
#[derive(Clone)]
pub struct AuthService {
    repo: Arc<dyn AuthRepo>,
    tokens: TokenService,
    mailer: Arc<dyn AuthMailer>,
    /// Hash verified against when no account matched, so a login for an unknown address
    /// costs the same as one for a known address with a wrong password.
    dummy_hash: Arc<String>,
}

impl std::fmt::Debug for AuthService {
    /// `dummy_hash` is a real Argon2id hash; it is not secret in the way a signing key
    /// is, but it is credential-shaped and there is no reason to print it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthService")
            .field("repo", &self.repo)
            .field("tokens", &self.tokens)
            .field("mailer", &self.mailer)
            .field("dummy_hash", &"[redacted]")
            .finish()
    }
}

/// Everything [`AuthService::new`] needs.
#[derive(Debug, Clone)]
pub struct AuthDeps {
    /// Storage.
    pub repo: Arc<dyn AuthRepo>,
    /// Token issuing and rotation.
    pub tokens: TokenService,
    /// Transactional mail, for the registration verification code.
    pub mailer: Arc<dyn AuthMailer>,
}

impl AuthService {
    /// Build the service.
    ///
    /// # Errors
    ///
    /// [`MenoError::Internal`] if the timing-equalising dummy hash cannot be computed.
    /// Failing startup is deliberate: a deployment that cannot produce one cannot
    /// equalise login timing, and running anyway would look fine until it was exploited.
    pub fn new(deps: AuthDeps) -> Result<Self, MenoError> {
        Ok(Self {
            repo: deps.repo,
            tokens: deps.tokens,
            mailer: deps.mailer,
            dummy_hash: Arc::new(password::dummy_hash()?),
        })
    }

    /// `POST /auth/register`.
    ///
    /// # Errors
    ///
    /// [`error::email_taken`] when the address is already registered, and whatever the
    /// repository reports for the rest. The verification code is sent on a best-effort
    /// basis — a mail failure is logged, never returned, because a client told "we could
    /// not send your code" learns whether the account exists.
    #[tracing::instrument(skip_all, fields(user_email = %redact(&req.email)))]
    pub async fn register(&self, req: &RegisterRequest) -> Result<AuthResponse, MenoError> {
        req.validate()?;

        let email = User::normalized_email(&req.email);
        // Hash before the insert, not after: a unique-violation on the address must not
        // cost a wasted Argon2id, and a hash that fails must not leave a row behind.
        let hash = password::hash_password(&req.password)?;

        let user = self
            .repo
            .create_user(NewUser {
                full_name: req.full_name.trim().to_owned(),
                email: email.clone(),
                password_hash: hash,
            })
            .await?;

        // Delegated rather than duplicated: storing-then-mailing is one decision, and a
        // second caller that did it slightly differently would be a second enumeration
        // rule.
        super::credentials::send_code(
            self.repo.as_ref(),
            self.mailer.as_ref(),
            &email,
            OtpType::VerifyEmail,
        )
        .await;

        self.issue(user).await
    }

    /// `POST /auth/login`.
    ///
    /// # Errors
    ///
    /// [`error::invalid_credentials`] for an unknown address *and* a wrong password —
    /// the same error, the same message and the same amount of work either way.
    #[tracing::instrument(skip_all, fields(user_email = %redact(&req.email)))]
    pub async fn login(&self, req: &LoginRequest) -> Result<AuthResponse, MenoError> {
        req.validate()?;

        let email = User::normalized_email(&req.email);
        let started = Instant::now();

        let Some(hash) = self.repo.find_password_hash(&email).await? else {
            // Named rather than a discarded `bool`: the point is the elapsed time, and a
            // caller holding a `false` is a caller who will eventually branch on it.
            password::spend_verification_time(req.password.clone(), self.dummy_hash.to_string())
                .await;
            tracing::debug!(
                elapsed_ms = started.elapsed().as_millis(),
                "no account matched"
            );
            return Err(error::invalid_credentials());
        };

        let matches = password::verify_password_async(req.password.clone(), hash).await;
        tracing::debug!(
            elapsed_ms = started.elapsed().as_millis(),
            "password checked"
        );

        if !matches {
            return Err(error::invalid_credentials());
        }

        let user = self
            .repo
            .find_user_by_email(&email)
            .await?
            .ok_or(error::invalid_credentials())?;

        self.issue(user).await
    }

    /// `POST /auth/refresh`.
    ///
    /// # Errors
    ///
    /// Whatever [`TokenService::refresh`] reports, including
    /// [`error::refresh_token_reused`] for a replayed token (§4.7 item 2).
    #[tracing::instrument(skip_all)]
    pub async fn refresh(&self, req: &RefreshTokenRequest) -> Result<AuthResponse, MenoError> {
        req.validate()?;

        let (pair, user) = self.tokens.refresh(&req.refresh_token).await?;
        let providers = self.repo.list_providers(user.id).await?;

        Ok(AuthResponse::new(
            pair.access_token,
            pair.refresh_token,
            self.tokens.access_ttl_secs(),
            user_response(&user, providers),
        ))
    }

    /// `POST /auth/logout`.
    ///
    /// # Errors
    ///
    /// Whatever the token service reports. A token that is already invalid is *not* an
    /// error: logging out twice has to succeed, or a client that retries on a timeout
    /// ends up permanently "logged in" with a stale token.
    #[tracing::instrument(skip_all)]
    pub async fn logout(&self, req: &LogoutRequest) -> Result<(), MenoError> {
        req.validate()?;

        if let Err(problem) = self
            .tokens
            .revoke(&req.refresh_token, req.access_token.as_deref())
            .await
        {
            // An unusable token means there is nothing left to revoke, which is the
            // state the caller asked for.
            if invalid_token_codes().contains(&problem.code()) {
                tracing::debug!(error = %problem, "logout with an already-dead token");
                return Ok(());
            }
            return Err(problem);
        }

        Ok(())
    }

    /// `POST /auth/logout-all` — "log out everywhere" (§4.7 item 3).
    ///
    /// # Errors
    ///
    /// Whatever the repository or the blocklist reports.
    #[tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn logout_everywhere(&self, user_id: Uuid) -> Result<(), MenoError> {
        self.tokens.revoke_all_for_user(user_id).await
    }

    /// `GET /auth/sessions` — the device list (§4.7 item 3).
    ///
    /// # Errors
    ///
    /// Whatever the repository reports.
    #[tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn list_sessions(&self, user_id: Uuid) -> Result<Vec<SessionResponse>, MenoError> {
        Ok(self
            .tokens
            .list_sessions(user_id)
            .await?
            .into_iter()
            .map(|session| SessionResponse {
                id: session.id,
                device_label: session.display_label().into_owned(),
                created_at: session.created_at,
                last_used_at: session.last_used_at,
                rotations: u32::from(session.rotated_from.is_some()),
            })
            .collect())
    }

    /// `POST /auth/sessions/{id}/revoke` (§4.7 item 3).
    ///
    /// # Errors
    ///
    /// [`error::session_not_found`] for an unknown id and for someone else's, so the
    /// endpoint cannot enumerate session ids.
    #[tracing::instrument(skip_all, fields(user_id = %user_id, session_id = %session_id))]
    pub async fn revoke_session(&self, user_id: Uuid, session_id: Uuid) -> Result<(), MenoError> {
        self.tokens.revoke_session(user_id, session_id).await
    }

    /// `POST /auth/google/callback` — the web flow.
    ///
    /// # Errors
    ///
    /// Whatever `exchange_code` reports: a rejected or replayed state is a
    /// [`MenoError::BadRequest`], an unverified address is
    /// [`error::email_not_verified`] (§7.12), and a provider fault is
    /// [`MenoError::Upstream`].
    #[tracing::instrument(skip_all)]
    pub async fn google_web_auth(
        &self,
        req: &GoogleWebAuthRequest,
        provider: &dyn GoogleExchange,
    ) -> Result<AuthResponse, MenoError> {
        req.validate()?;

        let identity = provider.exchange_code(&req.code, &req.state).await?;
        self.complete_google(identity, DeviceHint::default()).await
    }

    /// `POST /auth/google` — the mobile flow.
    ///
    /// # Errors
    ///
    /// Whatever `verify_id_token` reports. The device label is carried into the session
    /// so "your devices" names this one (§4.7 item 1).
    #[tracing::instrument(skip_all)]
    pub async fn google_mobile_auth(
        &self,
        req: &GoogleMobileAuthRequest,
        provider: &dyn GoogleExchange,
    ) -> Result<AuthResponse, MenoError> {
        req.validate()?;

        let identity = provider.verify_id_token(&req.id_token).await?;
        let hint = DeviceHint {
            device_label: req.device_label.clone(),
            user_agent: None,
        };
        self.complete_google(identity, hint).await
    }

    /// `GET /auth/google/url` — where to send the browser.
    ///
    /// # Errors
    ///
    /// [`error::provider_disabled`] when Google sign-in is switched off, and
    /// [`MenoError::Upstream`] when the provider is configured but unreachable.
    #[tracing::instrument(skip_all)]
    pub async fn google_authorize(
        &self,
        provider: &dyn GoogleExchange,
    ) -> Result<GoogleAuthorize, MenoError> {
        provider.authorize().await
    }

    /// Link-or-create for a verified Google identity (§4.7 item 7).
    ///
    /// The `email_verified` guard has already run inside [`GoogleExchange`]: an
    /// unverified provider email never reaches here, which is what stops this from being
    /// an account-takeover primitive. There is deliberately no second check — the seam is
    /// the check, and a second one would be a third place to remember it.
    async fn complete_google(
        &self,
        identity: GoogleIdentity,
        hint: DeviceHint,
    ) -> Result<AuthResponse, MenoError> {
        let subject = identity.subject().to_owned();

        if let Some(user) = self
            .repo
            .find_user_by_provider(AuthProvider::Google, &subject)
            .await?
        {
            return self.issue_with_device(user, hint).await;
        }

        // Not a known Google account. If the *address* is already ours, link it —
        // §4.7 item 7 permits this only because the provider verified the address.
        let email = User::normalized_email(identity.email());
        let user = match self.repo.find_user_by_email(&email).await? {
            Some(existing) => {
                self.repo
                    .link_provider(existing.id, AuthProvider::Google, &subject)
                    .await?
            }
            None => {
                self.repo
                    .create_user_from_provider(
                        NewUser {
                            full_name: identity.display_name(),
                            email,
                            // Empty because a Google-only account has no password. The
                            // repository writes NULL rather than an empty hash, and
                            // `find_password_hash` returns `None` — so a password login
                            // against such an account fails as "no such credentials"
                            // rather than as an Argon2 verification against "".
                            password_hash: String::new(),
                        },
                        AuthProvider::Google,
                        &subject,
                    )
                    .await?
            }
        };

        self.issue_with_device(user, hint).await
    }

    /// Mint a pair for `user` with no device context.
    async fn issue(&self, user: User) -> Result<AuthResponse, MenoError> {
        self.issue_with_device(user, DeviceHint::default()).await
    }

    /// Mint a pair for `user`, binding it to the device (§4.7 item 1).
    async fn issue_with_device(
        &self,
        user: User,
        hint: DeviceHint,
    ) -> Result<AuthResponse, MenoError> {
        let providers = self.repo.list_providers(user.id).await?;

        let pair = self
            .tokens
            .issue_pair(
                &user,
                providers.clone(),
                DeviceContext {
                    device_label: hint.device_label,
                    user_agent: hint.user_agent,
                    ip: None,
                },
            )
            .await?;

        Ok(AuthResponse::new(
            pair.access_token,
            pair.refresh_token,
            self.tokens.access_ttl_secs(),
            user_response(&user, providers),
        ))
    }
}

/// Mint a pair and project the account onto the wire shape.
///
/// Free rather than a method because [`super::credentials::CredentialService`] needs it
/// too — verify-email hands back a fresh session — and a duplicate of this in two files
/// is two places for `expires_in` to go missing.
pub(crate) async fn issue_response(
    tokens: &TokenService,
    repo: &dyn AuthRepo,
    user: User,
) -> Result<AuthResponse, MenoError> {
    let providers = repo.list_providers(user.id).await?;

    let pair = tokens
        .issue_pair(&user, providers.clone(), DeviceContext::default())
        .await?;

    Ok(AuthResponse::new(
        pair.access_token,
        pair.refresh_token,
        tokens.access_ttl_secs(),
        user_response(&user, providers),
    ))
}

/// Project a `User` onto the wire shape.
#[must_use]
pub fn user_response(user: &User, providers: Vec<AuthProvider>) -> UserResponse {
    UserResponse {
        id: user.id,
        full_name: user.full_name.clone(),
        bio: user.bio.clone(),
        email: user.email.clone(),
        verified: user.verified,
        avatar_id: user.avatar_id.clone(),
        avatar_url: user.avatar_url.clone(),
        providers,
        role: user.role(),
        created_at: user.created_at,
    }
}

/// An email address safe to put in a log line.
///
/// The local part is what identifies a person and what a breach list is keyed on, so
/// only the domain survives. A log line is not the place for an address.
#[must_use]
pub fn redact(address: &str) -> String {
    match address.rsplit_once('@') {
        Some((_, domain)) => format!("***@{domain}"),
        None => "***".to_owned(),
    }
}

/// The codes that mean "this token is not usable", which logout treats as success.
fn invalid_token_codes() -> [meno_core::ErrorCode; 3] {
    [
        meno_core::ErrorCode::InvalidToken,
        meno_core::ErrorCode::TokenExpired,
        meno_core::ErrorCode::RefreshTokenExpired,
    ]
}
