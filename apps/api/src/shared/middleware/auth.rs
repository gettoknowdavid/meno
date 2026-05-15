use crate::modules::auth::errors::AuthError;
use crate::modules::auth::jwt::decode_access_token;
use crate::modules::auth::model::{AccountProvider, UserRole};
use crate::state::MenoState;
use axum::extract::State;
use axum::{extract::Request, middleware::Next, response::Response};
use jsonwebtoken::errors::ErrorKind::ExpiredSignature;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct AuthUser {
    pub id: Uuid,
    pub jti: Uuid,
    pub full_name: String,
    pub email: String,
    pub verified: bool,
    pub account_provider: AccountProvider,
    pub role: UserRole,
}

pub async fn auth_middleware(
    State(app): State<MenoState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AuthError::MissingToken)?;

    let secret = &app.config.jwt_secret;
    let claims = decode_access_token(token, secret).map_err(|e| match e.kind() {
        ExpiredSignature => AuthError::AccessTokenExpired,
        _ => AuthError::InvalidToken,
    })?;

    if !claims.verified {
        return Err(AuthError::EmailNotVerified);
    }

    req.extensions_mut().insert(AuthUser {
        id: claims.sub,
        jti: claims.jti,
        full_name: claims.full_name,
        email: claims.email,
        verified: claims.verified,
        account_provider: claims.account_provider,
        role: claims.role,
    });

    Ok(next.run(req).await)
}
