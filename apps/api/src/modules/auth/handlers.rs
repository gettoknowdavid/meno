use crate::modules::auth::dto::{
    AuthResponse, ForgotPasswordRequest, GoogleMobileAuthRequest, GoogleUrlResponse,
    GoogleWebAuthRequest, LoginRequest, LogoutRequest, RefreshTokenRequest, RegisterRequest,
    ResendOtpRequest, ResetPasswordRequest, VerifyEmailRequest,
};
use crate::modules::auth::errors::AuthError;
use crate::shared::middleware::extractors::MenoBody;
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::extract::State;
use std::sync::Arc;
use validator::Validate;

pub async fn register(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<RegisterRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let user = app.auth.service.register(&body).await?;
    Ok(MenoResponse::created("Account created successfully", user))
}

pub async fn verify_email(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<VerifyEmailRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let user = app.auth.service.verify_email(&body).await?;
    Ok(MenoResponse::ok("Account verified successfully", user))
}

pub async fn resend_otp(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<ResendOtpRequest>,
) -> Result<MenoResponse<()>, AuthError> {
    body.validate()?;
    app.auth.service.resend_otp(&body).await?;
    Ok(MenoResponse::no_content("Verification email resent"))
}

pub async fn login(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<LoginRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let user = app.auth.service.login(&body).await?;
    Ok(MenoResponse::ok("Login successful", user))
}

pub async fn refresh(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<RefreshTokenRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let user = app.auth.service.refresh(&body).await?;
    Ok(MenoResponse::ok("Token refreshed successfully", user))
}

pub async fn logout(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<LogoutRequest>,
) -> Result<MenoResponse<()>, AuthError> {
    body.validate()?;
    app.auth.service.logout(&body).await?;
    Ok(MenoResponse::no_content("Logout successful"))
}

pub async fn forgot_password(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<ForgotPasswordRequest>,
) -> Result<MenoResponse<()>, AuthError> {
    body.validate()?;
    app.auth.service.forgot_password(&body).await?;
    Ok(MenoResponse::no_content("Password reset email sent"))
}

pub async fn reset_password(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<ResetPasswordRequest>,
) -> Result<MenoResponse<()>, AuthError> {
    body.validate()?;
    app.auth.service.reset_password(&body).await?;
    Ok(MenoResponse::no_content("Password reset successful"))
}

pub async fn google_auth_url(
    State(app): State<Arc<MenoState>>,
) -> Result<MenoResponse<GoogleUrlResponse>, AuthError> {
    let response = app.auth.service.google_authorize().await?;
    Ok(MenoResponse::ok("Google auth URL generated", response))
}

pub async fn google_web_callback(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<GoogleWebAuthRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let response = app.auth.service.google_web_auth(&body).await?;
    Ok(MenoResponse::ok("Google authentication success", response))
}

pub async fn google_mobile_auth(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<GoogleMobileAuthRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let response = app.auth.service.google_mobile_auth(&body).await?;
    Ok(MenoResponse::ok("Google authentication success", response))
}
