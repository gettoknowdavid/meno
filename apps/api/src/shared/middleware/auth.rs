use crate::modules::auth::errors::AuthError;
use crate::modules::auth::model::{AuthProvider, UserRole};
use crate::shared::services::redis::keys::RedisKey;
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

    let claims = app.jwt.decode_access(token)?;

    let blocklist_key = RedisKey::block_list("ACCESS_TOKEN", claims.jti);
    match app.redis.get::<String>(&blocklist_key).await {
        Ok(Some(_)) => return Err(AuthError::InvalidToken),
        Err(e) => return Err(AuthError::Redis(e)),
        Ok(None) => {}
    }

    if !claims.verified {
        return Err(AuthError::EmailNotVerified);
    }

    // Record the authenticated user in the parent HTTP span
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
