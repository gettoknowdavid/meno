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

pub async fn create_broadcast(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    MenoJson(body): MenoJson<dto::CreateBroadcastRequest>,
) -> Result<MenoResponse<dto::BroadcastResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn update_broadcast(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    MenoJson(body): MenoJson<dto::UpdateBroadcastRequest>,
) -> Result<MenoResponse<dto::BroadcastResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn delete_broadcast(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<()>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn go_live(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastSessionResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn join_broadcast(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastSessionResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn add_cohost(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    MenoJson(body): MenoJson<dto::AddCohostsRequest>,
) -> Result<MenoResponse<dto::CohostSessionResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn remove_cohost(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path((id, cohost_id)): Path<(Uuid, Uuid)>,
) -> Result<MenoResponse<()>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn get_broadcasts(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Query(params): Query<dto::BroadcastParams>,
) -> Result<MenoResponse<PaginationResponse<dto::BroadcastResponse>>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn get_broadcast(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn get_participants(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Query(params): Query<dto::ParticipantParams>,
) -> Result<MenoResponse<PaginationResponse<dto::ParticipantSummary>>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}

pub async fn refresh_token(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastSessionResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}
