use crate::config::MenoConfig;
use crate::modules::auth::dto::{
    AuthResponse, LoginRequest, RegisterRequest, ResendVerificationEmailRequest, VerifyEmailRequest,
};
use crate::modules::auth::errors::AuthError;
use crate::modules::auth::password::{hash_password, verify_password};
use crate::modules::auth::repository::AuthRepository;
use crate::shared::services::email::EmailService;
use crate::state::MenoState;
use time::{Duration, OffsetDateTime};

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

        let pwd_hash = tokio::task::spawn_blocking({
            let password = req.password.clone();
            move || hash_password(&password)
        })
        .await
        .map_err(|e| AuthError::Internal(e.into()))?
        .map_err(|_| AuthError::PasswordHash)?;

        let user = self.repo.create(&req, pwd_hash).await?;
        let email = user.email.clone();

        let otp = self.repo.set_verification_otp(&email).await?;

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
        let expires_at = OffsetDateTime::now_utc() + Duration::days(30);
        self.repo
            .store_refresh_token(jti, user.id, &refresh_token, expires_at)
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

        if !self.repo.verify_otp(&req.email, &req.code).await? {
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
        let expires_at = OffsetDateTime::now_utc() + Duration::days(30);
        self.repo
            .store_refresh_token(jti, user.id, &refresh_token, expires_at)
            .await?;

        Ok(AuthResponse {
            access_token,
            refresh_token,
            user: user.into_response_verified(),
        })
    }

    pub async fn resend_verification_email(
        &self,
        app: &MenoState,
        req: &ResendVerificationEmailRequest,
    ) -> Result<(), AuthError> {
        let user = match self.repo.find_by_email(&req.email).await? {
            Some(value) => value,
            None => return Ok(()),
        };

        if user.verified {
            return Err(AuthError::EmailAlreadyVerified);
        }

        if !self.repo.can_resend_verification_otp(&req.email).await? {
            return Err(AuthError::TooManyRequests);
        }

        self.repo.revoke_otp(&req.email).await?;

        let otp = self.repo.set_verification_otp(&req.email).await?;

        let html = verification_email_html(&user.full_name, &otp);
        self.send_email(&app.config, req.email.clone(), html).await;

        self.repo.set_resend_cooldown(&req.email).await?;

        Ok(())
    }

    async fn send_email(&self, config: &MenoConfig, to: String, html: String) -> () {
        let service = EmailService::new(&config);
        tokio::spawn(async move {
            if let Err(e) = service.send(&to, "Verify your Meno account", html).await {
                tracing::warn!(error = %e, "Failed to send verification email");
                tracing::info!(email = %to, "Verification email resent");
            }
        });
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
        let expires_at = OffsetDateTime::now_utc() + Duration::days(30);
        self.repo
            .store_refresh_token(jti, user.id, &refresh_token, expires_at)
            .await?;
        Ok(AuthResponse {
            access_token,
            refresh_token,
            user: user.into_response(),
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

// Concerning the verification via otp, here are some questions:
//
//
//
//
//
// I see the verify_email endpoint returns no data; if that is the case, how does the front-end automatically authenticate a registered user once verified, since the access & refresh token from the registration endpoint both carry old claims with verified=false​. I see the refresh​ endpoint, but I read somewhere that current industry standard leans towards sending the AuthResponse​ with on successful verification. Is this valid, and how does this pose any security risks? If the current method is superior, explain why.
//
//
//
// The login​ endpoint has a check for the user's verification; if false, it returns an early 403 error. Is this in support of good UI/UX? Isn't it better to allow the user log in but limit certain features until verified? I know this may be a bit more complex to code, but I believe it is better UX, so how would this change our current code and how would we ensure the main features are restricted until verification?
//
