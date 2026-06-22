use crate::modules::settings::dto;
use crate::modules::settings::errors::SettingsError;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::middleware::extractors::MenoBody;
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::Extension;
use axum::extract::State;
use std::sync::Arc;
use validator::Validate;

pub async fn get_settings(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
) -> Result<MenoResponse<dto::SettingsResponse>, SettingsError> {
    let response = app.settings.service.get(auth.id).await?;
    Ok(MenoResponse::ok("Settings retrieved", response))
}

pub async fn update_settings(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    MenoBody(body): MenoBody<dto::UpdateSettingsRequest>,
) -> Result<MenoResponse<dto::SettingsResponse>, SettingsError> {
    body.validate()?;
    let response = app.settings.service.update(auth.id, &body).await?;
    Ok(MenoResponse::ok("Settings updated", response))
}

pub async fn clear_push_token(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
) -> Result<MenoResponse<()>, SettingsError> {
    app.settings.service.clear_push_token(auth.id).await?;
    Ok(MenoResponse::no_content("Push token cleared"))
}
