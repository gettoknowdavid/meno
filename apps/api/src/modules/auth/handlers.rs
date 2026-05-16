use crate::modules::auth::dto::{
    AuthResponse, LoginRequest, RegisterRequest, ResendOtpRequest, VerifyEmailRequest,
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
    app.auth_service
        .resend_otp(&app, &body)
        .await?;
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
