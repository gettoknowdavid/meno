use crate::config::MenoConfig;
use crate::modules::auth::dto::{
    AuthResponse, ForgotPasswordRequest, GoogleMobileAuthRequest, GoogleUrlResponse,
    GoogleWebAuthRequest, LoginRequest, LogoutRequest, RefreshTokenRequest, RegisterRequest,
    ResendOtpRequest, ResetPasswordRequest, UserResponse, VerifyEmailRequest,
};
use crate::modules::auth::errors::AuthError;
use crate::modules::auth::jwt::verify_token_hash;
use crate::modules::auth::model::OtpType::{ResetPassword, VerifyEmail};
use crate::modules::auth::model::{AuthProvider, User};
use crate::modules::auth::password::{hash_password, verify_password};
use crate::modules::auth::repository::AuthRepository;
use crate::modules::auth::utils::generate_otp;
use crate::shared::integrations::google::GoogleUserInfo;
use crate::shared::services::email::EmailService;
use crate::shared::services::redis::RedisService;
use crate::state::MenoState;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct AuthService {
    repo: AuthRepository,
}
impl AuthService {
    pub fn new(database: sqlx::PgPool, redis: RedisService) -> Self {
        Self {
            repo: AuthRepository::new(database, redis),
        }
    }

    pub async fn register(
        &self,
        app: &MenoState,
        req: &RegisterRequest,
    ) -> Result<AuthResponse, AuthError> {
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
        self.repo.store_otp(&user.email, &otp, &VerifyEmail).await?;

        let html = verification_email_html(&user.full_name, &otp);
        self.send_email(&app.config, req.email.clone(), html).await;

        self.issue_tokens(app, &user).await
    }

    pub async fn verify_email(
        &self,
        app: &MenoState,
        req: &VerifyEmailRequest,
    ) -> Result<AuthResponse, AuthError> {
        let user = self
            .repo
            .find_by_email(&req.email)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        if user.verified {
            return Err(AuthError::EmailAlreadyVerified);
        }

        if !self
            .repo
            .verify_otp(&req.email, &req.code, &VerifyEmail)
            .await?
        {
            return Err(AuthError::InvalidOtp);
        }

        self.repo.set_verified(&req.email).await?;

        self.issue_tokens(app, &user).await
    }

    pub async fn resend_otp(
        &self,
        app: &MenoState,
        req: &ResendOtpRequest,
    ) -> Result<(), AuthError> {
        let user = match self.repo.find_by_email(&req.email).await? {
            Some(value) => value,
            None => return Ok(()),
        };

        match &req.otp_type {
            VerifyEmail if user.verified => return Err(AuthError::EmailAlreadyVerified),
            _ => {}
        };

        if !self.repo.can_resend_otp(&req.email).await? {
            return Err(AuthError::TooManyRequests);
        }

        self.repo
            .revoke_otp(&req.email, req.otp_type.clone())
            .await?;

        let otp = generate_otp();

        self.repo.store_otp(&req.email, &otp, &req.otp_type).await?;

        let html = match &req.otp_type {
            VerifyEmail => verification_email_html(&user.full_name, &otp),
            ResetPassword => reset_password_email_html(&user.full_name, &otp),
        };

        self.send_email(&app.config, req.email.clone(), html).await;
        self.repo.set_resend_cooldown(&req.email).await?;
        Ok(())
    }

    pub async fn login(
        &self,
        app: &MenoState,
        req: &LoginRequest,
    ) -> Result<AuthResponse, AuthError> {
        // Adds tracing spans for security auditing.
        let span = tracing::info_span!("auth.login", email = %req.email);
        let _guard = span.enter();

        let identity = self
            .repo
            .find_identity(&AuthProvider::Email, &req.email)
            .await?
            .ok_or(AuthError::InvalidCredentials)?;

        self.repo.check_login_rate_limit(&req.email).await?;

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

        let _ = self.repo.clear_login_rate_limit(&req.email).await;
        tracing::info!(user_id = %user.id, "login.success");

        self.issue_tokens(app, &user).await
    }

    pub async fn forgot_password(
        &self,
        app: &MenoState,
        req: &ForgotPasswordRequest,
    ) -> Result<(), AuthError> {
        let user = match self.repo.find_by_email(&req.email).await? {
            Some(u) => u,
            None => return Ok(()),
        };
        let otp = generate_otp();
        self.repo
            .store_otp(&req.email, &otp, &ResetPassword)
            .await?;
        let html = reset_password_email_html(&user.full_name, &otp);
        self.send_email(&app.config, req.email.clone(), html).await;
        Ok(())
    }

    pub async fn reset_password(
        &self,
        app: &MenoState,
        req: &ResetPasswordRequest,
    ) -> Result<(), AuthError> {
        let user = match self.repo.find_by_email(&req.email).await? {
            Some(value) => value,
            None => return Ok(()),
        };
        if !self
            .repo
            .verify_otp(&user.email, &req.code, &ResetPassword)
            .await?
        {
            return Err(AuthError::InvalidOtp);
        }
        let pwd_hash = self.spawn_hash_pwd(req.new_password.clone()).await?;
        self.repo.update_password(user.id, pwd_hash).await?;
        self.repo.revoke_otp(&user.email, ResetPassword).await?;
        self.repo.revoke_all_refresh_tokens(user.id).await?;
        self.repo
            .block_all_user_access_tokens(user.id, app.config.access_token_expiration)
            .await?;
        Ok(())
    }

    pub async fn logout(&self, app: &MenoState, req: &LogoutRequest) -> Result<(), AuthError> {
        let claims = &app.jwt.decode_refresh(&req.refresh_token)?;
        self.repo.revoke_refresh_token(claims.jti).await?;

        if let Some(ref access_token) = req.access_token {
            if let Ok(access_claims) = app.jwt.decode_access(access_token) {
                let now = OffsetDateTime::now_utc().unix_timestamp();
                let remaining_secs = access_claims.exp.saturating_sub(now);
                if remaining_secs > 0 {
                    self.repo
                        .block_access_token(access_claims.jti, remaining_secs)
                        .await?;
                }
            }
        }

        app.redis
            .invalidate_all_user_keys(claims.sub)
            .await
            .map_err(|e| AuthError::Internal(anyhow::anyhow!(e)))?;

        Ok(())
    }

    pub async fn refresh(
        &self,
        app: &MenoState,
        req: &RefreshTokenRequest,
    ) -> Result<AuthResponse, AuthError> {
        let claims = app.jwt.decode_refresh(&req.refresh_token)?;

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

        let providers = app
            .profile_service
            .find_user_providers(user.id)
            .await
            .map_err(|e| AuthError::Internal(anyhow::anyhow!(e)))?;

        let access_token = app.jwt.sign_access(
            user.id,
            &user.email,
            &user.full_name,
            user.verified,
            providers.clone(),
            user.role.clone(),
        )?;

        let (new_refresh_token, new_jti) = app.jwt.sign_refresh(user.id)?;

        self.repo
            .rotate_refresh_token(
                user.id,
                claims.jti,
                new_jti,
                &new_refresh_token,
                app.config.refresh_token_expiration,
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

    pub async fn google_authorize(&self, app: &MenoState) -> Result<GoogleUrlResponse, AuthError> {
        let (url, csrf_token, pkce_code_verifier) = app.google.authorize_url();
        self.repo
            .store_oauth_state(csrf_token.secret(), pkce_code_verifier.secret())
            .await?;
        Ok(GoogleUrlResponse {
            url: url.to_string(),
        })
    }

    pub async fn google_web_auth(
        &self,
        app: &MenoState,
        req: &GoogleWebAuthRequest,
    ) -> Result<AuthResponse, AuthError> {
        let raw_verifier = self.repo.consumes_oauth_state(&req.state).await?;
        let pkce_code_verifier = oauth2::PkceCodeVerifier::new(raw_verifier);

        let userinfo = app
            .google
            .exchange_code(req.code.clone(), pkce_code_verifier)
            .await
            .map_err(|e| AuthError::GoogleAuthFailed(e.to_string()))?;

        self.upsert_google_user(app, &userinfo).await
    }

    pub async fn google_mobile_auth(
        &self,
        app: &MenoState,
        req: &GoogleMobileAuthRequest,
    ) -> Result<AuthResponse, AuthError> {
        let userinfo = app
            .google
            .verify_id_token(&req.id_token)
            .await
            .map_err(|e| AuthError::GoogleAuthFailed(e.to_string()))?;

        self.upsert_google_user(app, &userinfo).await
    }

    // Helper functions
    async fn send_email(&self, config: &MenoConfig, to: String, html: String) -> () {
        let service = EmailService::new(&config);
        tokio::spawn(async move {
            if let Err(e) = service.send(&to, "Verify your Meno account", html).await {
                tracing::warn!(error = %e, "Failed to send verification email");
                tracing::info!(email = %to, "Verification email resent");
            }
        });
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
        app: &MenoState,
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

        self.issue_tokens(app, &user).await
    }
    async fn issue_tokens(&self, app: &MenoState, user: &User) -> Result<AuthResponse, AuthError> {
        let providers = app
            .profile_service
            .find_user_providers(user.id)
            .await
            .map_err(|e| AuthError::Internal(anyhow::anyhow!(e)))?;

        let access_token = app.jwt.sign_access(
            user.id,
            &user.email,
            &user.full_name,
            user.verified,
            providers.clone(),
            user.role.clone(),
        )?;

        let (refresh_token, jti) = app.jwt.sign_refresh(user.id)?;

        self.repo
            .store_refresh_token(
                jti,
                user.id,
                &refresh_token,
                app.config.refresh_token_expiration,
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

fn verification_email_html(full_name: &str, otp: &str) -> String {
    format!(
        r#"
            <!DOCTYPE html>
            <html>
            <body style="margin:0;padding:0;background:#0f0f1a;font-family:sans-serif;">
              <div style="max-width:480px;margin:40px auto;background:#1a1a2e;border-radius:16px;padding:40px;text-align:center;">
                <h1 style="color:#ffffff;font-size:24px;margin-bottom:8px;">Verify your email</h1>
                <p style="color:#a0a0b8;margin-bottom:32px;">Hi {full_name}, enter this code in the app to verify your account.</p>
                <div style="background:#2a2a3e;border-radius:12px;padding:24px;margin-bottom:32px;">
                  <span style="color:#7c3aed;font-size:48px;font-weight:700;letter-spacing:12px;">{otp}</span>
                </div>
                <p style="color:#606080;font-size:14px;">This code expires in 15 minutes.<br/>If you didn't create a Meno account, ignore this email.</p>
              </div>
            </body>
            </html>
            "#,
        full_name = full_name,
        otp = otp
    )
}
fn reset_password_email_html(full_name: &str, otp: &str) -> String {
    format!(
        r#"
        <!DOCTYPE html>
            <html>
            <body style="margin:0;padding:0;background:#0f0f1a;font-family:sans-serif;">
              <div style="max-width:480px;margin:40px auto;background:#1a1a2e;border-radius:16px;padding:40px;text-align:center;">
                <h1 style="color:#ffffff;font-size:24px;margin-bottom:8px;">Reset your password</h1>

                <p style="color:#a0a0b8;margin-bottom:32px;">
                    Hi {full_name},<br>
                    You requested to reset your Meno account password.
                </p>

                <div style="background:#2a2a3e;border-radius:12px;padding:24px;margin-bottom:32px;">
                  <span style="color:#7c3aed;font-size:48px;font-weight:700;letter-spacing:12px;">{otp}</span>
                </div>

                <p style="color:#606080;font-size:14px;">
                    This code expires in 15 minutes.<br>
                    If you didn't request a password reset, please ignore this email.
                </p>

                <p style="color:#606080;font-size:13px;margin-top:32px;">
                    For security reasons, never share this code with anyone.
                </p>
              </div>
            </body>
            </html>
            "#,
        full_name = full_name,
        otp = otp
    )
}
