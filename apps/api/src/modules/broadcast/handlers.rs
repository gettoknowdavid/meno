use crate::modules::broadcast::dto;
use crate::modules::broadcast::errors::BroadcastError;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::middleware::json_rejection::MenoJson;
use crate::shared::pagination::PaginationResponse;
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::Extension;
use axum::extract::{Path, Query, State};
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

pub async fn create_broadcast(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    MenoJson(body): MenoJson<dto::CreateBroadcastRequest>,
) -> Result<MenoResponse<dto::BroadcastResponse>, BroadcastError> {
    body.validate()?;
    let creator_id = auth_user.id;
    let broadcast = state.broadcast.create(&state, body, creator_id).await?;
    Ok(MenoResponse::created("Broadcast created", broadcast))
}

pub async fn update_broadcast(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    MenoJson(body): MenoJson<dto::UpdateBroadcastRequest>,
) -> Result<MenoResponse<dto::BroadcastResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn delete_broadcast(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<()>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn go_live(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastSessionResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn join_broadcast(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastSessionResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn add_cohost(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    MenoJson(body): MenoJson<dto::AddCohostsRequest>,
) -> Result<MenoResponse<dto::CohostSessionResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn remove_cohost(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path((id, cohost_id)): Path<(Uuid, Uuid)>,
) -> Result<MenoResponse<()>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn get_broadcasts(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Query(params): Query<dto::BroadcastParams>,
) -> Result<MenoResponse<PaginationResponse<dto::BroadcastResponse>>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn get_broadcast(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn get_participants(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Query(params): Query<dto::ParticipantParams>,
) -> Result<MenoResponse<PaginationResponse<dto::ParticipantSummary>>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn refresh_token(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastSessionResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}
