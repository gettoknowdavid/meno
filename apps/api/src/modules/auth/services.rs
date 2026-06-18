use crate::config::Config;
use crate::jobs::Jobs;
use crate::jobs::email_jobs::{SendEmailJob, reset_pwd_email_html, verify_email_html};
use crate::modules::auth::cache::AuthCache;
use crate::modules::auth::dto::{
    AuthResponse, ForgotPasswordRequest, GoogleMobileAuthRequest, GoogleUrlResponse,
    GoogleWebAuthRequest, LoginRequest, LogoutRequest, RefreshTokenRequest, RegisterRequest,
    ResendOtpRequest, ResetPasswordRequest, UserResponse, VerifyEmailRequest,
};
use crate::modules::auth::errors::AuthError;
use crate::modules::auth::jwt::Jwt;
use crate::modules::auth::jwt::verify_token_hash;
use crate::modules::auth::model::OtpType::{ResetPassword, VerifyEmail};
use crate::modules::auth::model::{AuthProvider, OtpType, User};
use crate::modules::auth::password::{hash_password, verify_password};
use crate::modules::auth::repository::AuthRepository;
use crate::modules::auth::utils::generate_otp;
use crate::shared::integrations::google::GoogleAuthService;
use crate::shared::integrations::google::GoogleUserInfo;
use crate::shared::services::redis::Redis;
use sqlx::PgPool;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthService {
    repo: AuthRepository,
    cache: AuthCache,
    jobs: Jobs,
    jwt: Jwt,
    config: Arc<Config>,
    google: GoogleAuthService,
}
impl AuthService {
    pub fn new(db: PgPool, redis: Redis, jobs: Jobs, config: Arc<Config>, jwt: Jwt) -> Self {
        Self {
            repo: AuthRepository::new(db),
            cache: AuthCache::new(redis),
            google: GoogleAuthService::new(&config),
            config,
            jwt,
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

        let pwd_hash = self.spawn_hash_pwd(req.password.clone()).await?;

        let user = self.repo.create_user_tx(&req.full_name, &req.email).await?;

        self.repo
            .create_identity(user.id, &AuthProvider::Email, &user.email, Some(&pwd_hash))
            .await?;

        let otp = generate_otp();
        self.cache
            .store_otp(&user.email, &otp, &VerifyEmail)
            .await?;

        self.push_email(
            req.email.clone(),
            user.full_name.clone(),
            otp.clone(),
            &VerifyEmail,
        )
        .await;

        self.issue_tokens(&user).await
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
            .verify_otp(&req.email, &req.code, &VerifyEmail)
            .await?
        {
            return Err(AuthError::InvalidOtp);
        }

        self.repo.set_verified(&req.email).await?;

        self.issue_tokens(&user).await
    }

    pub async fn resend_otp(&self, req: &ResendOtpRequest) -> Result<(), AuthError> {
        let user = match self.repo.find_by_email(&req.email).await? {
            Some(value) => value,
            None => return Ok(()),
        };

        match &req.otp_type {
            VerifyEmail if user.verified => return Err(AuthError::EmailAlreadyVerified),
            _ => {}
        };

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

        self.push_email(
            req.email.clone(),
            user.full_name.clone(),
            otp.clone(),
            &req.otp_type,
        )
        .await;

        self.cache.set_resend_cooldown(&req.email).await?;
        Ok(())
    }

    pub async fn login(&self, req: &LoginRequest) -> Result<AuthResponse, AuthError> {
        // Adds tracing spans for security auditing.
        let span = tracing::info_span!("auth.login", email = %req.email);
        let _guard = span.enter();

        let identity = self
            .repo
            .find_identity(&AuthProvider::Email, &req.email)
            .await?
            .ok_or(AuthError::InvalidCredentials)?;

        self.cache.check_login_rate_limit(&req.email).await?;

        let password_hash = identity
            .password_hash
            .as_deref()
            .ok_or(AuthError::InvalidCredentials)?;

        if !verify_password(&req.password, password_hash) {
            tracing::warn!(email = %req.email, "login.invalid_credentials");
            return Err(AuthError::InvalidCredentials);
        }

        let user = self
            .repo
            .find_by_id(identity.user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        let _ = self.cache.clear_login_rate_limit(&req.email).await;
        tracing::info!(user_id = %user.id, "login.success");

        self.issue_tokens(&user).await
    }

    pub async fn forgot_password(&self, req: &ForgotPasswordRequest) -> Result<(), AuthError> {
        let user = match self.repo.find_by_email(&req.email).await? {
            Some(u) => u,
            None => return Ok(()),
        };

        let otp = generate_otp();

        self.cache
            .store_otp(&req.email, &otp, &ResetPassword)
            .await?;

        self.push_email(
            req.email.clone(),
            user.full_name.clone(),
            otp.clone(),
            &ResetPassword,
        )
        .await;

        Ok(())
    }

    pub async fn reset_password(&self, req: &ResetPasswordRequest) -> Result<(), AuthError> {
        let user = match self.repo.find_by_email(&req.email).await? {
            Some(value) => value,
            None => return Ok(()),
        };
        if !self
            .cache
            .verify_otp(&user.email, &req.code, &ResetPassword)
            .await?
        {
            return Err(AuthError::InvalidOtp);
        }
        let pwd_hash = self.spawn_hash_pwd(req.new_password.clone()).await?;
        self.repo.update_password(user.id, pwd_hash).await?;
        self.cache.revoke_otp(&user.email, ResetPassword).await?;
        self.repo.revoke_all_refresh_tokens(user.id).await?;
        self.cache
            .block_all_user_access_tokens(user.id, self.config.access_token_expiration)
            .await?;
        Ok(())
    }

    pub async fn logout(&self, req: &LogoutRequest) -> Result<(), AuthError> {
        let claims = &self.jwt.decode_refresh(&req.refresh_token)?;
        self.repo.revoke_refresh_token(claims.jti).await?;

        if let Some(ref access_token) = req.access_token {
            if let Ok(access_claims) = self.jwt.decode_access(access_token) {
                let now = OffsetDateTime::now_utc().unix_timestamp();
                let remaining_secs = access_claims.exp.saturating_sub(now);
                if remaining_secs > 0 {
                    self.cache
                        .block_access_token(access_claims.jti, remaining_secs)
                        .await?;
                }
            }
        }

        self.cache.invalidate_all_user_keys(claims.sub).await?;

        Ok(())
    }

    pub async fn refresh(&self, req: &RefreshTokenRequest) -> Result<AuthResponse, AuthError> {
        let claims = self.jwt.decode_refresh(&req.refresh_token)?;

        let user = match self.repo.find_by_id(claims.sub).await? {
            None => return Err(AuthError::UserNotFound),
            Some(value) => value,
        };

        let stored = self
            .repo
            .find_refresh_token(claims.jti, claims.sub)
            .await?
            .ok_or(AuthError::RefreshTokenNotFound)?;

        if !verify_token_hash(&req.refresh_token, &stored.token_hash) {
            return Err(AuthError::InvalidToken);
        }

        // Check DB-level expiry for extra security
        if stored.expires_at < OffsetDateTime::now_utc() {
            self.repo.revoke_refresh_token(claims.jti).await?;
            return Err(AuthError::RefreshTokenExpired);
        }

        let providers = self
            .repo
            .find_providers(user.id)
            .await
            .map_err(|e| AuthError::Internal(anyhow::anyhow!(e)))?;

        let access_token = self.jwt.sign_access(
            user.id,
            &user.email,
            &user.full_name,
            user.verified,
            providers.clone(),
            user.role.clone(),
        )?;

        let (new_refresh_token, new_jti) = self.jwt.sign_refresh(user.id)?;

        self.repo
            .rotate_refresh_token(
                user.id,
                claims.jti,
                new_jti,
                &new_refresh_token,
                self.config.refresh_token_expiration,
            )
            .await?;

        Ok(AuthResponse {
            access_token,
            refresh_token: new_refresh_token,
            user: UserResponse {
                id: user.id,
                full_name: user.full_name.clone(),
                bio: user.bio.clone(),
                email: user.email.clone(),
                verified: user.verified,
                avatar_id: user.avatar_id.clone(),
                avatar_url: user.avatar_url.clone(),
                providers,
                created_at: user.created_at,
                deleted_at: user.deleted_at,
            },
        })
    }

    pub async fn google_authorize(&self) -> Result<GoogleUrlResponse, AuthError> {
        let (url, csrf_token, pkce_code_verifier) = self.google.authorize_url();
        self.cache
            .store_oauth_state(csrf_token.secret(), pkce_code_verifier.secret())
            .await?;
        Ok(GoogleUrlResponse {
            url: url.to_string(),
        })
    }

    pub async fn google_web_auth(
        &self,
        req: &GoogleWebAuthRequest,
    ) -> Result<AuthResponse, AuthError> {
        let raw_verifier = self.cache.consumes_oauth_state(&req.state).await?;
        let pkce_code_verifier = oauth2::PkceCodeVerifier::new(raw_verifier);

        let userinfo = self
            .google
            .exchange_code(req.code.clone(), pkce_code_verifier)
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

    pub async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>, AuthError> {
        self.repo.find_by_id(id).await
    }

    // Helper functions
    async fn push_email(&self, to: String, name: String, otp: String, otp_type: &OtpType) {
        let (subject, html) = match otp_type {
            VerifyEmail => verify_email_html(&name, &otp),
            ResetPassword => reset_pwd_email_html(&name, &otp),
        };

        self.jobs
            .push_email(SendEmailJob { to, subject, html })
            .await
            .unwrap_or_else(|e| tracing::warn!(error=%e, "Failed to queue email"))
    }
    async fn spawn_hash_pwd(&self, password: String) -> Result<String, AuthError> {
        tokio::task::spawn_blocking({
            let password = password.clone();
            move || hash_password(&password)
        })
        .await
        .map_err(|e| AuthError::Internal(e.into()))?
        .map_err(|_| AuthError::PasswordHash)
    }
    async fn upsert_google_user(
        &self,
        userinfo: &GoogleUserInfo,
    ) -> Result<AuthResponse, AuthError> {
        let existing_identity = self
            .repo
            .find_identity(&AuthProvider::Google, &userinfo.sub)
            .await?;

        let user = if let Some(identity) = existing_identity {
            self.repo
                .find_by_id(identity.user_id)
                .await?
                .ok_or(AuthError::UserNotFound)?
        } else {
            let existing_user = self.repo.find_by_email(&userinfo.email).await?;
            if let Some(user) = existing_user {
                self.repo
                    .link_provider(user.id, &AuthProvider::Google, &userinfo.sub)
                    .await?;
                user
            } else {
                let user = self
                    .repo
                    .create_user_tx(&userinfo.name, &userinfo.email)
                    .await?;

                self.repo
                    .create_identity(user.id, &AuthProvider::Google, &user.email, None)
                    .await?;

                if userinfo.email_verified {
                    self.repo.set_verified(&userinfo.email).await?;
                }

                user
            }
        };

        self.issue_tokens(&user).await
    }
    async fn issue_tokens(&self, user: &User) -> Result<AuthResponse, AuthError> {
        let providers = self
            .repo
            .find_providers(user.id)
            .await
            .map_err(|e| AuthError::Internal(anyhow::anyhow!(e)))?;

        let access_token = self.jwt.sign_access(
            user.id,
            &user.email,
            &user.full_name,
            user.verified,
            providers.clone(),
            user.role.clone(),
        )?;

        let (refresh_token, jti) = self.jwt.sign_refresh(user.id)?;

        self.repo
            .store_refresh_token(
                jti,
                user.id,
                &refresh_token,
                self.config.refresh_token_expiration,
            )
            .await?;

        Ok(AuthResponse {
            access_token,
            refresh_token,
            user: UserResponse {
                id: user.id,
                full_name: user.full_name.clone(),
                bio: user.bio.clone(),
                email: user.email.clone(),
                verified: user.verified,
                avatar_id: user.avatar_id.clone(),
                avatar_url: user.avatar_url.clone(),
                providers,
                created_at: user.created_at,
                deleted_at: user.deleted_at,
            },
        })
    }
}
