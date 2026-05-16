use crate::modules::auth::dto::{AuthResponse, RegisterRequest};
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
