//! The settings endpoints (plan §9.3).
//!
//! Parse → validate → delegate → respond, the same three-line handler shape as
//! `auth`, `broadcast` and `profile`. The guard that supplies `Extension<AuthUser>`
//! is applied in [`crate::routes::settings`], never here.

use axum::extract::{Extension, State};

use super::dto::{SettingsResponse, UpdateSettingsRequest, Validatable};
use super::state::SettingsState;
use crate::middleware::auth::AuthUser;
use crate::middleware::extractors::MenoBody;
use crate::types::meno_response::{MenoResponse, Outcome};

/// `GET /settings` — the caller's switches.
pub async fn get_settings(
    State(state): State<SettingsState>,
    Extension(auth): Extension<AuthUser>,
) -> Outcome<SettingsResponse> {
    let settings = state.service.get(auth.id).await?;
    Ok(MenoResponse::ok("Settings retrieved", settings))
}

/// `PATCH /settings` — a partial update; the response is the fresh row.
pub async fn update_settings(
    State(state): State<SettingsState>,
    Extension(auth): Extension<AuthUser>,
    MenoBody(body): MenoBody<UpdateSettingsRequest>,
) -> Outcome<SettingsResponse> {
    body.validate()?;
    let settings = state.service.update(auth.id, &body).await?;
    Ok(MenoResponse::ok("Settings updated", settings))
}

/// `DELETE /settings/push-token` — unset the device token.
///
/// `no_content` in the envelope sense — no `data` key — not HTTP 204, matching what
/// `master` sent so a client already decoding this answer keeps decoding it.
pub async fn clear_push_token(
    State(state): State<SettingsState>,
    Extension(auth): Extension<AuthUser>,
) -> Outcome<()> {
    state.service.clear_push_token(auth.id).await?;
    Ok(MenoResponse::no_content("Push token cleared"))
}
