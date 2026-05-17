use crate::modules::auth::dto::{
    AuthResponse, ForgotPasswordRequest, LoginRequest, LogoutRequest, RefreshTokenRequest,
    RegisterRequest, ResendOtpRequest, ResetPasswordRequest, VerifyEmailRequest,
};
use crate::modules::auth::errors::AuthError;
use crate::shared::middleware::json_rejection::MenoJson;
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::extract::State;
use std::sync::Arc;
use validator::Validate;

pub async fn register(
    State(app): State<Arc<MenoState>>,
    MenoJson(body): MenoJson<RegisterRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let user = app.auth_service.register(&app, &body).await?;
    Ok(MenoResponse::created("Account created successfully", user))
}

pub async fn verify_email(
    State(app): State<Arc<MenoState>>,
    MenoJson(body): MenoJson<VerifyEmailRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let user = app.auth_service.verify_email(&app, &body).await?;
    Ok(MenoResponse::ok("Account created successfully", user))
}

pub async fn resend_otp(
    State(app): State<Arc<MenoState>>,
    MenoJson(body): MenoJson<ResendOtpRequest>,
) -> Result<MenoResponse<()>, AuthError> {
    body.validate()?;
    app.auth_service.resend_otp(&app, &body).await?;
    Ok(MenoResponse::no_content("Verification email resent"))
}

pub async fn login(
    State(app): State<Arc<MenoState>>,
    MenoJson(body): MenoJson<LoginRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let user = app.auth_service.login(&app, &body).await?;
    Ok(MenoResponse::ok("Login successful", user))
}

pub async fn refresh(
    State(app): State<Arc<MenoState>>,
    MenoJson(body): MenoJson<RefreshTokenRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let user = app.auth_service.refresh(&app, &body).await?;
    Ok(MenoResponse::ok("Token refreshed successfully", user))
}

pub async fn logout(
    State(app): State<Arc<MenoState>>,
    MenoJson(body): MenoJson<LogoutRequest>,
) -> Result<MenoResponse<()>, AuthError> {
    body.validate()?;
    app.auth_service.logout(&app, &body).await?;
    Ok(MenoResponse::no_content("Logout successful"))
}

pub async fn forgot_password(
    State(app): State<Arc<MenoState>>,
    MenoJson(body): MenoJson<ForgotPasswordRequest>,
) -> Result<MenoResponse<()>, AuthError> {
    body.validate()?;
    app.auth_service.forgot_password(&app, &body).await?;
    Ok(MenoResponse::no_content("Password reset email sent"))
}

pub async fn reset_password(
    State(app): State<Arc<MenoState>>,
    MenoJson(body): MenoJson<ResetPasswordRequest>,
) -> Result<MenoResponse<()>, AuthError> {
    body.validate()?;
    app.auth_service.reset_password(&body).await?;
    Ok(MenoResponse::no_content("Password reset successful"))
}
