use crate::jobs::Jobs;
use crate::jobs::email_jobs::{SendEmailJob, reset_pwd_email_html, verify_email_html};
use crate::modules::auth::cache::AuthCache;
use crate::modules::auth::dto::{
    AuthResponse, ForgotPasswordRequest, GoogleMobileAuthRequest, GoogleUrlResponse,
    GoogleWebAuthRequest, LoginRequest, LogoutRequest, RefreshTokenRequest, RegisterRequest,
    ResendOtpRequest, ResetPasswordRequest, UserResponse, VerifyEmailRequest,
};
use crate::modules::auth::errors::AuthError;
use crate::modules::auth::model::{AuthProvider, OtpType, User};
use crate::modules::auth::repository::AuthRepo;
use crate::modules::auth::token::{IssuedTokenPair, TokenService};
use crate::shared::integrations::google::GoogleAuth;
use crate::shared::integrations::google::GoogleUserInfo;
use std::sync::Arc;

/// `AuthService` orchestrates auth flows. It holds its dependencies directly.
/// It never takes `&MenoState`.
#[derive(Clone)]
pub struct AuthService {
    repo: Arc<dyn AuthRepo>,
    cache: Arc<dyn AuthCache>,
    tokens: Arc<TokenService>,
    google: Arc<GoogleAuth>,
    jobs: Jobs,
}

impl AuthService {
    pub fn new(
        repo: Arc<dyn AuthRepo>,
        cache: Arc<dyn AuthCache>,
        tokens: Arc<TokenService>,
        google: Arc<GoogleAuth>,
        jobs: Jobs,
    ) -> Self {
        Self {
            repo,
            cache,
            tokens,
            google,
            jobs,
        }
    }

    pub async fn register(&self, req: &RegisterRequest) -> Result<AuthResponse, AuthError> {
        let existing = self
            .repo
            .find_identity(&AuthProvider::Email, &req.email)
            .await?;

        if existing.is_some() {
            return Err(AuthError::EmailTaken);
        }

        let pwd_hash = hash_password_async(req.password.clone()).await?;
        let user = self.repo.create_user(&req.full_name, &req.email).await?;
        self.repo
            .create_identity(user.id, &AuthProvider::Email, &user.email, Some(&pwd_hash))
            .await?;

        let otp = generate_otp();
        self.cache
            .store_otp(&user.email, &otp, &OtpType::VerifyEmail)
            .await?;
        self.push_email(&user.email, &user.full_name, &otp, &OtpType::VerifyEmail);

        // New users have only the email provider
        let pair = self
            .tokens
            .issue_pair(&user, vec![AuthProvider::Email])
            .await?;
        Ok(build_auth_response(user, pair, vec![AuthProvider::Email]))
    }

    pub async fn verify_email(&self, req: &VerifyEmailRequest) -> Result<AuthResponse, AuthError> {
        let user = self
            .repo
            .find_by_email(&req.email)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        if user.verified {
            return Err(AuthError::EmailAlreadyVerified);
        }

        if !self
            .cache
            .verify_otp(&req.email, &req.code, &OtpType::VerifyEmail)
            .await?
        {
            return Err(AuthError::InvalidOtp);
        }

        self.repo.set_verified(&req.email).await?;

        // Re-fetch to get updated verified flag
        let user = self
            .repo
            .find_by_id(user.id)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        let providers = vec![AuthProvider::Email];
        let pair = self.tokens.issue_pair(&user, providers.clone()).await?;
        Ok(build_auth_response(user, pair, providers))
    }

    pub async fn resend_otp(&self, req: &ResendOtpRequest) -> Result<(), AuthError> {
        // Return Ok even if user not found (don't leak email existence)
        let Some(user) = self.repo.find_by_email(&req.email).await? else {
            return Ok(());
        };

        if matches!(req.otp_type, OtpType::VerifyEmail) && user.verified {
            return Err(AuthError::EmailAlreadyVerified);
        }

        if !self.cache.can_resend_otp(&req.email).await? {
            return Err(AuthError::TooManyRequests);
        }

        self.cache
            .revoke_otp(&req.email, req.otp_type.clone())
            .await?;
        let otp = generate_otp();
        self.cache
            .store_otp(&req.email, &otp, &req.otp_type)
            .await?;
        self.push_email(&user.email, &user.full_name, &otp, &req.otp_type);
        self.cache.set_resend_cooldown(&req.email).await?;
        Ok(())
    }

    pub async fn login(&self, req: &LoginRequest) -> Result<AuthResponse, AuthError> {
        let identity = self
            .repo
            .find_identity(&AuthProvider::Email, &req.email)
            .await?
            .ok_or(AuthError::InvalidCredentials)?;

        self.cache.check_login_rate_limit(&req.email).await?;

        let hash = identity
            .password_hash
            .as_deref()
            .ok_or(AuthError::InvalidCredentials)?;

        if !verify_password_sync(&req.password, hash) {
            tracing::warn!(email = %req.email, "login.invalid_credentials");
            return Err(AuthError::InvalidCredentials);
        }

        let user = self
            .repo
            .find_by_id(identity.user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        if !user.verified {
            return Err(AuthError::EmailNotVerified);
        }

        let _ = self.cache.clear_login_rate_limit(&req.email).await;
        tracing::info!(user_id = %user.id, "login.success");

        // Fetch all linked providers for the access token claims
        let providers = self.get_providers(user.id).await?;
        let pair = self.tokens.issue_pair(&user, providers.clone()).await?;
        Ok(build_auth_response(user, pair, providers))
    }

    pub async fn logout(&self, req: &LogoutRequest) -> Result<(), AuthError> {
        self.tokens
            .revoke(&req.refresh_token, req.access_token.as_deref())
            .await?;
        Ok(())
    }

    pub async fn refresh(&self, req: &RefreshTokenRequest) -> Result<AuthResponse, AuthError> {
        let providers = self.get_providers_from_refresh(&req.refresh_token).await?;
        let (pair, user) = self
            .tokens
            .rotate(&req.refresh_token, providers.clone())
            .await?;
        Ok(build_auth_response(user, pair, providers))
    }

    pub async fn forgot_password(&self, req: &ForgotPasswordRequest) -> Result<(), AuthError> {
        let Some(user) = self.repo.find_by_email(&req.email).await? else {
            return Ok(());
        };

        let otp = generate_otp();
        self.cache
            .store_otp(&req.email, &otp, &OtpType::ResetPassword)
            .await?;
        self.push_email(&user.email, &user.full_name, &otp, &OtpType::ResetPassword);
        Ok(())
    }

    pub async fn reset_password(&self, req: &ResetPasswordRequest) -> Result<(), AuthError> {
        let Some(user) = self.repo.find_by_email(&req.email).await? else {
            return Ok(());
        };

        if !self
            .cache
            .verify_otp(&user.email, &req.code, &OtpType::ResetPassword)
            .await?
        {
            return Err(AuthError::InvalidOtp);
        }

        let hash = hash_password_async(req.new_password.clone()).await?;
        self.repo.update_password(user.id, hash).await?;
        self.cache
            .revoke_otp(&user.email, OtpType::ResetPassword)
            .await?;

        // Revoke all sessions — password changed
        self.tokens.revoke_all_for_user(user.id).await?;
        Ok(())
    }

    pub async fn google_authorize(&self) -> Result<GoogleUrlResponse, AuthError> {
        let (url, csrf_token, pkce_verifier) = self.google.authorize_url();
        self.cache
            .store_oauth_state(csrf_token.secret(), pkce_verifier.secret())
            .await?;
        Ok(GoogleUrlResponse {
            url: url.to_string(),
        })
    }

    pub async fn google_web_auth(
        &self,
        req: &GoogleWebAuthRequest,
    ) -> Result<AuthResponse, AuthError> {
        let raw_verifier = self.cache.consume_oauth_state(&req.state).await?;
        let pkce = oauth2::PkceCodeVerifier::new(raw_verifier);
        let userinfo = self
            .google
            .exchange_code(req.code.clone(), pkce)
            .await
            .map_err(|e| AuthError::GoogleAuthFailed(e.to_string()))?;
        self.upsert_google_user(&userinfo).await
    }

    pub async fn google_mobile_auth(
        &self,
        req: &GoogleMobileAuthRequest,
    ) -> Result<AuthResponse, AuthError> {
        let userinfo = self
            .google
            .verify_id_token(&req.id_token)
            .await
            .map_err(|e| AuthError::GoogleAuthFailed(e.to_string()))?;
        self.upsert_google_user(&userinfo).await
    }

    /// Exposed for the auth middleware — no DB call on the hot path.
    pub fn decode_access_claims(
        &self,
        token: &str,
    ) -> Result<crate::modules::auth::token::AccessClaims, AuthError> {
        self.tokens.decode_access(token)
    }

    pub async fn find_user_by_id(&self, id: uuid::Uuid) -> Result<Option<User>, AuthError> {
        self.repo.find_by_id(id).await
    }

    pub async fn cleanup_expired_refresh_tokens(&self) -> Result<u64, AuthError> {
        self.repo.cleanup_expired_refresh_tokens().await
    }

    async fn upsert_google_user(&self, info: &GoogleUserInfo) -> Result<AuthResponse, AuthError> {
        let existing = self
            .repo
            .find_identity(&AuthProvider::Google, &info.sub)
            .await?;

        let user = if let Some(identity) = existing {
            self.repo
                .find_by_id(identity.user_id)
                .await?
                .ok_or(AuthError::UserNotFound)?
        } else {
            if let Some(user) = self.repo.find_by_email(&info.email).await? {
                self.repo
                    .link_provider(user.id, &AuthProvider::Google, &info.sub)
                    .await?;

                user
            } else {
                let user = self.repo.create_user(&info.name, &info.email).await?;

                self.repo
                    .create_identity(user.id, &AuthProvider::Google, &user.email, None)
                    .await?;

                if info.email_verified {
                    self.repo.set_verified(&info.email).await?;
                }

                self.repo
                    .find_by_id(user.id)
                    .await?
                    .ok_or(AuthError::UserNotFound)?
            }
        };

        let providers = self.get_providers(user.id).await?;
        let pair = self.tokens.issue_pair(&user, providers.clone()).await?;
        Ok(build_auth_response(user, pair, providers))
    }

    async fn get_providers(&self, user_id: uuid::Uuid) -> Result<Vec<AuthProvider>, AuthError> {
        self.repo.find_user_providers(user_id).await
    }

    async fn get_providers_from_refresh(
        &self,
        refresh_token: &str,
    ) -> Result<Vec<AuthProvider>, AuthError> {
        let claims = self.tokens.decode_refresh(refresh_token)?;
        self.get_providers(claims.sub).await
    }

    fn push_email(&self, to: &str, name: &str, otp: &str, otp_type: &OtpType) {
        let (subject, html) = match otp_type {
            OtpType::VerifyEmail => verify_email_html(name, otp),
            OtpType::ResetPassword => reset_pwd_email_html(name, otp),
        };
        let job = SendEmailJob {
            to: to.to_string(),
            subject,
            html,
        };
        let jobs = self.jobs.clone();
        tokio::spawn(async move {
            jobs.push_email(job)
                .await
                .unwrap_or_else(|e| tracing::warn!(error=%e, "Failed to queue email"));
        });
    }
}

pub(crate) fn generate_otp() -> String {
    use rand::RngExt;
    format!("{:06}", rand::rng().random_range(100_000_u32..=999_999))
}

async fn hash_password_async(password: String) -> Result<String, AuthError> {
    tokio::task::spawn_blocking(move || crate::modules::auth::password::hash_password(&password))
        .await
        .map_err(|e| AuthError::Internal(e.into()))?
        .map_err(|_| AuthError::PasswordHash)
}

fn verify_password_sync(password: &str, hash: &str) -> bool {
    crate::modules::auth::password::verify_password(password, hash)
}

fn build_auth_response(
    user: User,
    pair: IssuedTokenPair,
    providers: Vec<AuthProvider>,
) -> AuthResponse {
    AuthResponse {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        user: UserResponse {
            id: user.id,
            full_name: user.full_name,
            bio: user.bio,
            email: user.email,
            verified: user.verified,
            avatar_id: user.avatar_id,
            avatar_url: user.avatar_url,
            providers,
            created_at: user.created_at,
            deleted_at: user.deleted_at,
        },
    }
}
