use crate::modules::profile::dto;
use crate::modules::profile::dto::PublicProfileResponse;
use crate::modules::profile::errors::ProfileError;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::middleware::extractors::MenoBody;
use crate::shared::pagination::PaginationResponse;
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::extract::{Extension, Path, Query, State};
use std::sync::Arc;
use validator::Validate;

pub async fn get_me(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<MenoResponse<dto::MeResponse>, ProfileError> {
    let me = app.profile.get_me(auth_user.id).await?;
    Ok(MenoResponse::ok("Profile retrieved", me))
}

pub async fn get_profile(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<uuid::Uuid>,
) -> Result<MenoResponse<PublicProfileResponse>, ProfileError> {
    let user = app.profile.get_user_by_id(auth_user.id, id).await?;
    Ok(MenoResponse::ok("Profile retrieved", user))
}

pub async fn get_avatar_upload_url(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Query(params): Query<dto::AvatarUploadUrlParams>,
) -> Result<MenoResponse<dto::AvatarUploadUrlResponse>, ProfileError> {
    let response = app
        .profile
        .get_avatar_upload_url(&app, auth_user.id, &params.content_type)
        .await?;
    Ok(MenoResponse::ok("Upload URL generated", response))
}

pub async fn update_me(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    MenoBody(body): MenoBody<dto::UpdateProfileRequest>,
) -> Result<MenoResponse<dto::MeResponse>, ProfileError> {
    body.validate()?;
    let me = app.profile.update_me(auth_user.id, &body).await?;
    Ok(MenoResponse::ok("Profile updated", me))
}

pub async fn search_profiles(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Query(params): Query<dto::ProfileSearchParam>,
) -> Result<MenoResponse<PaginationResponse<dto::ProfileSearchResult>>, ProfileError> {
    params.validate()?;
    let results = app
        .profile
        .search_profiles(auth_user.id, &params)
        .await?;
    Ok(MenoResponse::ok("Profiles retrieved successfully", results))
}
