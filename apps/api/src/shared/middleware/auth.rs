use crate::modules::auth::errors::AuthError;
use crate::modules::auth::model::{AuthProvider, UserRole};
use crate::state::MenoState;
use axum::{extract::Request, extract::State, middleware::Next, response::Response};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct AuthUser {
    pub id: Uuid,
    pub jti: Uuid,
    pub full_name: String,
    pub email: String,
    pub verified: bool,
    pub providers: Vec<AuthProvider>,
    pub role: UserRole,
}

pub async fn auth_middleware(
    State(app): State<Arc<MenoState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let token = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AuthError::MissingToken)?;

    // Fast path: pure JWT decode — no DB, no Redis
    let claims = app.auth.tokens.decode_access(token)?;

    // Redis blocklist check — one network round trip
    if app
        .auth
        .tokens
        .is_access_token_blocked(claims.jti, claims.sub, claims.iat)
        .await?
    {
        return Err(AuthError::InvalidToken);
    }

    if !claims.verified {
        return Err(AuthError::EmailNotVerified);
    }

    tracing::Span::current().record("user_id", claims.sub.to_string().as_str());

    req.extensions_mut().insert(AuthUser {
        id: claims.sub,
        jti: claims.jti,
        full_name: claims.full_name,
        email: claims.email,
        verified: claims.verified,
        providers: claims.providers,
        role: claims.role,
    });

    Ok(next.run(req).await)
}
