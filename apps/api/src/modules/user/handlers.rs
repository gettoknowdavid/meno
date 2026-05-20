use crate::modules::user::dto;
use crate::modules::user::errors::UserError;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::middleware::json_rejection::MenoJson;
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::extract::{Extension, Query, State};
use std::sync::Arc;
use validator::Validate;

pub async fn get_me(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<MenoResponse<dto::MeResponse>, UserError> {
    let me = app.user_service.get_me(auth_user.id).await?;
    Ok(MenoResponse::ok("User profile retrieved", me))
}

pub async fn get_avatar_upload_url(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Query(params): Query<dto::AvatarUploadUrlParams>,
) -> Result<MenoResponse<dto::AvatarUploadUrlResponse>, UserError> {
    let response = app
        .user_service
        .get_avatar_upload_url(&app, auth_user.id, &params.content_type)
        .await?;
    Ok(MenoResponse::ok("Upload URL generated", response))
}

pub async fn update_me(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    MenoJson(body): MenoJson<dto::UpdateProfileRequest>,
) -> Result<MenoResponse<dto::MeResponse>, UserError> {
    body.validate()?;
    let me = app.user_service.update_me(auth_user.id, &body).await?;
    Ok(MenoResponse::ok("Profile updated", me))
}
