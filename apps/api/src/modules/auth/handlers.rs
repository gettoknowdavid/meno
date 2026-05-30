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
    let user = app.auth.register(&app, &body).await?;
    Ok(MenoResponse::created("Account created successfully", user))
}

pub async fn verify_email(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<VerifyEmailRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let user = app.auth.verify_email(&app, &body).await?;
    Ok(MenoResponse::ok("Account verified successfully", user))
}

pub async fn resend_otp(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<ResendOtpRequest>,
) -> Result<MenoResponse<()>, AuthError> {
    body.validate()?;
    app.auth.resend_otp(&app, &body).await?;
    Ok(MenoResponse::no_content("Verification email resent"))
}

pub async fn login(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<LoginRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let user = app.auth.login(&app, &body).await?;
    Ok(MenoResponse::ok("Login successful", user))
}

pub async fn refresh(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<RefreshTokenRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let user = app.auth.refresh(&app, &body).await?;
    Ok(MenoResponse::ok("Token refreshed successfully", user))
}

pub async fn logout(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<LogoutRequest>,
) -> Result<MenoResponse<()>, AuthError> {
    body.validate()?;
    app.auth.logout(&app, &body).await?;
    Ok(MenoResponse::no_content("Logout successful"))
}

pub async fn forgot_password(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<ForgotPasswordRequest>,
) -> Result<MenoResponse<()>, AuthError> {
    body.validate()?;
    app.auth.forgot_password(&app, &body).await?;
    Ok(MenoResponse::no_content("Password reset email sent"))
}

pub async fn reset_password(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<ResetPasswordRequest>,
) -> Result<MenoResponse<()>, AuthError> {
    body.validate()?;
    app.auth.reset_password(&app, &body).await?;
    Ok(MenoResponse::no_content("Password reset successful"))
}

pub async fn google_auth_url(
    State(app): State<Arc<MenoState>>,
) -> Result<MenoResponse<GoogleUrlResponse>, AuthError> {
    let response = app.auth.google_authorize(&app).await?;
    Ok(MenoResponse::ok("Google auth URL generated", response))
}

pub async fn google_web_callback(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<GoogleWebAuthRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let response = app.auth.google_web_auth(&app, &body).await?;
    Ok(MenoResponse::ok("Google authentication success", response))
}

pub async fn google_mobile_auth(
    State(app): State<Arc<MenoState>>,
    MenoBody(body): MenoBody<GoogleMobileAuthRequest>,
) -> Result<MenoResponse<AuthResponse>, AuthError> {
    body.validate()?;
    let response = app.auth.google_mobile_auth(&app, &body).await?;
    Ok(MenoResponse::ok("Google authentication success", response))
}
