use crate::modules::broadcast::dto;
use crate::modules::broadcast::errors::BroadcastError;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::middleware::extractors::{MenoBody, MenoJson, MenoQuery};
use crate::shared::pagination::CursorPage;
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::Extension;
use axum::extract::{Path, State};
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

pub async fn create_broadcast(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    MenoBody(body): MenoBody<dto::CreateBroadcastRequest>,
) -> Result<MenoResponse<dto::BroadcastResponse>, BroadcastError> {
    body.validate()?;
    let broadcast = app.broadcast.service.create(body, auth.id).await?;
    Ok(MenoResponse::created("Broadcast created", broadcast))
}

pub async fn update_broadcast(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    MenoBody(body): MenoBody<dto::UpdateBroadcastRequest>,
) -> Result<MenoResponse<dto::BroadcastResponse>, BroadcastError> {
    body.validate()?;
    if id.is_nil() {
        return Err(BroadcastError::InvalidId);
    }
    let broadcast = app.broadcast.service.update(body, id, auth.id).await?;
    Ok(MenoResponse::created("Broadcast updated", broadcast))
}

pub async fn delete_broadcast(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<()>, BroadcastError> {
    if id.is_nil() {
        return Err(BroadcastError::InvalidId);
    }
    app.broadcast.service.delete(id, auth.id).await?;
    Ok(MenoResponse::no_content("Broadcast deleted successfully"))
}

pub async fn go_live(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastSessionResponse>, BroadcastError> {
    let session = app.broadcast.service.start(id, auth.id).await?;
    Ok(MenoResponse::created("Broadcast started", session))
}

pub async fn end_broadcast(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::EndBroadcastResponse>, BroadcastError> {
    let response = app.broadcast.service.end(id, auth.id).await?;
    Ok(MenoResponse::created("Broadcast ended", response))
}

pub async fn join_broadcast(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastSessionResponse>, BroadcastError> {
    if id.is_nil() {
        return Err(BroadcastError::InvalidId);
    }
    let response = app.broadcast.service.join(id, auth.id).await?;
    Ok(MenoResponse::ok("Broadcast joined", response))
}

pub async fn leave_broadcast(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::LeaveBroadcastResponse>, BroadcastError> {
    if id.is_nil() {
        return Err(BroadcastError::InvalidId);
    }
    let response = app.broadcast.service.leave(id, auth.id).await?;
    Ok(MenoResponse::ok("Broadcast left", response))
}

pub async fn add_cohost(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    MenoBody(body): MenoBody<dto::AddCohostRequest>,
) -> Result<MenoResponse<dto::CohostSessionResponse>, BroadcastError> {
    let response = app
        .broadcast
        .service
        .add_cohost(id, auth.id, body.cohost)
        .await?;
    Ok(MenoResponse::ok("Cohost added successfully", response))
}

pub async fn remove_cohost(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path((id, cohost_id)): Path<(Uuid, Uuid)>,
    MenoJson(body): MenoJson<dto::RemoveCohostRequest>,
) -> Result<MenoResponse<()>, BroadcastError> {
    let remove_from_room = body.remove_from_room.unwrap_or(false);
    app.broadcast
        .service
        .remove_cohost(id, cohost_id, auth.id, remove_from_room)
        .await?;
    Ok(MenoResponse::no_content("Cohost removed successfully"))
}

pub async fn get_broadcast(
    State(app): State<Arc<MenoState>>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastResponse>, BroadcastError> {
    if id.is_nil() {
        return Err(BroadcastError::InvalidId);
    }
    let response = app.broadcast.service.get_broadcast(id).await?;
    Ok(MenoResponse::ok("Broadcast retrieved", response))
}

pub async fn get_broadcasts(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    MenoQuery(query): MenoQuery<dto::BroadcastQuery>,
) -> Result<MenoResponse<CursorPage<dto::BroadcastListItem>>, BroadcastError> {
    let page = app
        .broadcast
        .service
        .get_broadcasts(&query, Some(auth.id))
        .await?;
    Ok(MenoResponse::ok("Broadcasts retrieved", page))
}

pub async fn get_participants(
    State(app): State<Arc<MenoState>>,
    Path(id): Path<Uuid>,
    MenoQuery(query): MenoQuery<dto::ParticipantQuery>,
) -> Result<MenoResponse<CursorPage<dto::ParticipantListItem>>, BroadcastError> {
    let page = app.broadcast.service.get_participants(id, &query).await?;
    Ok(MenoResponse::ok("Participants retrieved", page))
}

pub async fn get_live_participants(
    State(app): State<Arc<MenoState>>,
    Path(id): Path<Uuid>,
    MenoQuery(query): MenoQuery<dto::ParticipantQuery>,
) -> Result<MenoResponse<CursorPage<dto::ParticipantListItem>>, BroadcastError> {
    let page = app
        .broadcast
        .service
        .get_live_participants(id, &query)
        .await?;
    Ok(MenoResponse::ok("Live participants retrieved", page))
}

pub async fn refresh_token(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::BroadcastRefreshTokenResponse>, BroadcastError> {
    if id.is_nil() {
        return Err(BroadcastError::InvalidId);
    }
    let response = app.broadcast.service.refresh_token(id, auth.id).await?;
    Ok(MenoResponse::ok("Token refreshed", response))
}
