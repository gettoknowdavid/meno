use crate::modules::broadcast::dto;
use crate::modules::broadcast::errors::BroadcastError;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::middleware::extractors::{MenoBody, MenoJson};
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
    Extension(auth): Extension<AuthUser>,
    MenoBody(body): MenoBody<dto::CreateBroadcastRequest>,
) -> Result<MenoResponse<dto::BroadcastResponse>, BroadcastError> {
    body.validate()?;
    let broadcast = state.broadcast.create(&state, body, auth.id).await?;
    Ok(MenoResponse::created("Broadcast created", broadcast))
}

pub async fn update_broadcast(
    State(state): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    MenoBody(body): MenoBody<dto::UpdateBroadcastRequest>,
) -> Result<MenoResponse<dto::BroadcastResponse>, BroadcastError> {
    body.validate()?;
    if id.is_nil() {
        return Err(BroadcastError::InvalidId);
    }
    let broadcast = state.broadcast.update(&state, body, id, auth.id).await?;
    Ok(MenoResponse::created("Broadcast updated", broadcast))
}

pub async fn delete_broadcast(
    State(state): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<()>, BroadcastError> {
    if id.is_nil() {
        return Err(BroadcastError::InvalidId);
    }
    state.broadcast.delete(id, auth.id).await?;
    Ok(MenoResponse::no_content("Broadcast deleted successfully"))
}

pub async fn go_live(
    State(state): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastSessionResponse>, BroadcastError> {
    let session = state.broadcast.start(&state, id, auth.id).await?;
    Ok(MenoResponse::created("Broadcast started", session))
}

pub async fn end_broadcast(
    State(state): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::EndBroadcastResponse>, BroadcastError> {
    let response = state.broadcast.end(&state, id, auth.id).await?;
    Ok(MenoResponse::created("Broadcast ended", response))
}

pub async fn join_broadcast(
    State(state): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastSessionResponse>, BroadcastError> {
    if id.is_nil() {
        return Err(BroadcastError::InvalidId);
    }
    let response = state.broadcast.join(&state, id, auth.id).await?;
    Ok(MenoResponse::ok("Broadcast joined", response))
}

pub async fn leave_broadcast(
    State(state): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::LeaveBroadcastResponse>, BroadcastError> {
    if id.is_nil() {
        return Err(BroadcastError::InvalidId);
    }
    let response = state.broadcast.leave(id, auth.id).await?;
    Ok(MenoResponse::ok("Broadcast left", response))
}

pub async fn add_cohost(
    State(state): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    MenoBody(body): MenoBody<dto::AddCohostRequest>,
) -> Result<MenoResponse<dto::CohostSessionResponse>, BroadcastError> {
    let response = state
        .broadcast
        .add_cohost(&state, id, auth.id, body.cohost)
        .await?;
    Ok(MenoResponse::ok("Cohost added successfully", response))
}

pub async fn remove_cohost(
    State(state): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path((id, cohost_id)): Path<(Uuid, Uuid)>,
    MenoJson(body): MenoJson<dto::RemoveCohostRequest>,
) -> Result<MenoResponse<()>, BroadcastError> {
    let remove_from_room = body.remove_from_room.unwrap_or(false);
    state
        .broadcast
        .remove_cohost(&state, id, cohost_id, auth.id, remove_from_room)
        .await?;
    Ok(MenoResponse::no_content("Cohost removed successfully"))
}

pub async fn get_broadcast(
    State(state): State<Arc<MenoState>>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastResponse>, BroadcastError> {
    if id.is_nil() {
        return Err(BroadcastError::InvalidId);
    }
    let response = state.broadcast.get_broadcast(id).await?;
    Ok(MenoResponse::ok("Broadcast retrieved", response))
}

pub async fn get_broadcasts(
    State(state): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Query(params): Query<dto::BroadcastParams>,
) -> Result<MenoResponse<PaginationResponse<dto::BroadcastListItem>>, BroadcastError> {
    let page = state
        .broadcast
        .get_broadcasts(&params, Some(auth.id))
        .await?;
    Ok(MenoResponse::ok("Broadcasts retrieved", page))
}

pub async fn get_participants(
    State(state): State<Arc<MenoState>>,
    Path(id): Path<Uuid>,
    Query(params): Query<dto::ParticipantParams>,
) -> Result<MenoResponse<PaginationResponse<dto::ParticipantListItem>>, BroadcastError> {
    let page = state.broadcast.get_participants(&params, id).await?;
    Ok(MenoResponse::ok("Participants retrieved", page))
}

pub async fn get_live_participants(
    State(state): State<Arc<MenoState>>,
    Path(id): Path<Uuid>,
    Query(params): Query<dto::ParticipantParams>,
) -> Result<MenoResponse<PaginationResponse<dto::ParticipantListItem>>, BroadcastError> {
    let page = state.broadcast.get_live_participants(&params, id).await?;
    Ok(MenoResponse::ok("Live participants retrieved", page))
}

pub async fn refresh_token(
    State(state): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastSessionResponse>, BroadcastError> {
    Err(BroadcastError::AlreadyCohost)
}
