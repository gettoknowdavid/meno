use crate::modules::auth::errors::AuthError;
use crate::shared::middleware::auth::AuthUser;
use axum::Extension;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

pub async fn require_verified(
    Extension(auth_user): Extension<AuthUser>,
    req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    if !auth_user.verified {
        return Err(AuthError::EmailNotVerified);
    }
    Ok(next.run(req).await)
}
