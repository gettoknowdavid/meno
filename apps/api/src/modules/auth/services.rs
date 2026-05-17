use crate::config::MenoConfig;
use crate::modules::auth::dto::{
    AuthResponse, ForgotPasswordRequest, LoginRequest, LogoutRequest, RegisterRequest,
    ResendOtpRequest, ResetPasswordRequest, VerifyEmailRequest,
};
use crate::modules::auth::errors::AuthError;
use crate::modules::auth::model::OtpType::{ResetPassword, VerifyEmail};
use crate::modules::auth::password::{hash_password, verify_password};
use crate::modules::auth::repository::AuthRepository;
use crate::shared::services::email::EmailService;
use crate::state::MenoState;

#[derive(Clone)]
pub struct AuthService {
    repo: AuthRepository,
}
impl AuthService {
    pub fn new(db: sqlx::PgPool, rd: fred::clients::Pool, env: &str) -> Self {
        Self {
            repo: AuthRepository::new(db, rd, env),
        }
    }

    pub async fn register(
        &self,
        app: &MenoState,
        req: &RegisterRequest,
    ) -> Result<AuthResponse, AuthError> {
        if self.repo.user_exists(&req.email).await? {
            return Err(AuthError::EmailTaken);
        }

        let pwd_hash = self.spawn_hash_pwd(req.password.clone()).await?;

        let user = self.repo.create(&req, pwd_hash).await?;
        let email = user.email.clone();

        let otp = self.repo.store_otp(&email, VerifyEmail).await?;

        let html = verification_email_html(&user.full_name, &otp);
        self.send_email(&app.config, req.email.clone(), html).await;

        let access_token = app.jwt.sign_access(
            user.id,
            &user.full_name,
            &user.email,
            user.verified,
            user.account_provider.clone(),
            user.role.clone(),
        )?;

        let (refresh_token, jti) = app.jwt.sign_refresh(user.id)?;
        self.repo
            .store_refresh_token(jti, user.id, &refresh_token)
            .await?;
        Ok(AuthResponse {
            access_token,
            refresh_token,
            user: user.into_response(),
        })
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

        if !self.repo.verify_otp(&req.email, &req.code, &VerifyEmail).await? {
            return Err(AuthError::InvalidOtp);
        }

        self.repo.set_verified(&req.email).await?;

        let access_token = app.jwt.sign_access(
            user.id,
            &user.full_name,
            &user.email,
            true,
            user.account_provider.clone(),
            user.role.clone(),
        )?;

        let (refresh_token, jti) = app.jwt.sign_refresh(user.id)?;
        self.repo
            .store_refresh_token(jti, user.id, &refresh_token)
            .await?;

        Ok(AuthResponse {
            access_token,
            refresh_token,
            user: user.into_response_verified(),
        })
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
        let otp = self
            .repo
            .store_otp(&req.email, req.otp_type.clone())
            .await?;

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
        let user = match self.repo.find_by_email(&req.email).await? {
            None => return Err(AuthError::InvalidCredentials),
            Some(value) => value,
        };
        if !verify_password(&req.password, &user.password) {
            return Err(AuthError::InvalidCredentials);
        }
        let access_token = app.jwt.sign_access(
            user.id,
            &user.email,
            &user.full_name,
            user.verified,
            user.account_provider.clone(),
            user.role.clone(),
        )?;
        let (refresh_token, jti) = app.jwt.sign_refresh(user.id)?;
        self.repo
            .store_refresh_token(jti, user.id, &refresh_token)
            .await?;
        Ok(AuthResponse {
            access_token,
            refresh_token,
            user: user.into_response(),
        })
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
        let otp = self.repo.store_otp(&req.email, ResetPassword).await?;
        let html = reset_password_email_html(&user.full_name, &otp);
        self.send_email(&app.config, req.email.clone(), html).await;
        Ok(())
    }

    pub async fn reset_password(&self, req: &ResetPasswordRequest) -> Result<(), AuthError> {
        let user = match self.repo.find_by_email(&req.email).await? {
            Some(value) => value,
            None => return Ok(()),
        };
        if !self.repo.verify_otp(&user.email, &req.code, &ResetPassword).await? {
            return Err(AuthError::InvalidOtp);
        }
        let pwd_hash = self.spawn_hash_pwd(req.new_password.clone()).await?;
        self.repo.update_password(&user.email, pwd_hash).await?;
        self.repo.revoke_otp(&user.email, ResetPassword).await?;
        self.repo.revoke_all_refresh_tokens(user.id).await?;
        Ok(())
    }

    pub async fn logout(&self, app: &MenoState, req: &LogoutRequest) -> Result<(), AuthError> {
        let claims = &app.jwt.decode_refresh(&req.refresh_token)?;
        self.repo.revoke_refresh_token(claims.jti).await?;
        Ok(())
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
