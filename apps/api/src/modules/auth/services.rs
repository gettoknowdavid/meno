use crate::modules::auth::dto::{AuthResponse, RegisterRequest};
use crate::modules::auth::errors::AuthError;
use crate::modules::auth::password::hash_password;
use crate::modules::auth::repository::AuthRepository;
use crate::shared::services::email::EmailService;
use crate::state::MenoState;

#[derive(Clone)]
pub struct AuthService {
    repo: AuthRepository,
}
impl AuthService {
    pub fn new(db: sqlx::PgPool, rd: fred::clients::Pool) -> Self {
        Self {
            repo: AuthRepository::new(db, rd),
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

        let otp = self.repo.set_verification_otp(&user.email).await?;
        let email_service = EmailService::new(&app.config);
        let email_html = email_service.verification_email_html(&user.full_name, &otp);
        if let Err(e) = email_service
            .send(&user.email, "Verify your Meno account", email_html)
            .await
        {
            tracing::warn!(error = %e, email = %user.email, "Failed to send verification email");
        }

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
}
