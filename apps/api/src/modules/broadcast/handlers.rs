//! The broadcast endpoints (plan §9.3).
//!
//! # Thin on purpose
//!
//! Each function is parse → validate → delegate → respond. The same rule as
//! [`crate::modules::auth::handlers`], for the same reason: the decisions these
//! endpoints make — is the caller the creator, is it live, may they join — are the
//! ones that have to be testable without HTTP. A handler with an `if` in it that is not
//! "which extractor did I get" is a rule that belongs in [`super::service`].
//!
//! # The envelope
//!
//! Every function returns `Result<MenoResponse<T>, Failure>`, whose error field is one
//! [`MenoError`] and is rendered by the crate's single §4.2 renderer. The
//! `Response::from` impl on `Result` is what lets a handler return that shape directly:
//! it is axum's own `Infallible`-shaped impl, not a bespoke one.
//!
//! # Dates
//!
//! Nothing here formats a timestamp. Every date on these endpoints is written by
//! [`super::dto`] with `#[serde(with = "time::serde::rfc3339")]`, so a date cannot be a
//! nine-number tuple by accident; `apps/api/tests/wire_dates.rs` enforces it.

use axum::extract::{Extension, Path, Query, State};
use uuid::Uuid;

use super::dto::{
    AddCohostRequest, BroadcastQuery, BroadcastSessionResponse, CreateBroadcastRequest,
    EndBroadcastResponse, LeaveBroadcastResponse, ParticipantQuery, RemoveCohostRequest,
    UpdateBroadcastRequest, Validatable,
};
use super::service::{BroadcastService, summary_of};
use crate::middleware::auth::AuthUser;
use crate::types::meno_response::{MenoResponse, Outcome};
use meno_core::CursorPage;

/// The broadcast module's rules, as a handler sees them.
#[derive(Clone)]
pub struct Handlers {
    service: BroadcastService,
}

impl std::fmt::Debug for Handlers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handlers")
            .field("service", &self.service)
            .finish()
    }
}

impl From<BroadcastService> for Handlers {
    fn from(service: BroadcastService) -> Self {
        Self { service }
    }
}

/// `POST /broadcasts` — create.
///
/// # Errors
///
/// 422 with the field map when the body is unacceptable; 400 for a bad time zone or a
/// past `start_time`; 404 for a cohost id with no live account.
pub async fn create(
    State(handlers): State<Handlers>,
    Extension(auth): Extension<AuthUser>,
    axum::Json(request): axum::Json<CreateBroadcastRequest>,
) -> Outcome<super::dto::BroadcastResponse> {
    request.validate()?;
    let actor = summary_of(&auth);

    let broadcast = handlers.service.create(request, actor).await?;
    Ok(MenoResponse::created("Broadcast created", broadcast))
}

/// `GET /broadcasts` — a page of broadcasts.
///
/// # Errors
///
/// 400 `INVALID_CURSOR` when `?cursor=` cannot be decoded.
pub async fn list(
    State(handlers): State<Handlers>,
    Query(query): Query<BroadcastQuery>,
) -> Outcome<CursorPage<super::dto::BroadcastListItem>> {
    let page = handlers.service.list(&query).await?;
    Ok(MenoResponse::ok("Broadcasts retrieved", page))
}

/// `GET /broadcasts/{id}` — one broadcast.
///
/// # Errors
///
/// 404 when no live broadcast carries that id.
pub async fn get(
    State(handlers): State<Handlers>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Outcome<super::dto::BroadcastResponse> {
    let viewer = summary_of(&auth);

    let broadcast = handlers.service.get(id, Some(&viewer)).await?;
    Ok(MenoResponse::ok("Broadcast retrieved", broadcast))
}

/// `PATCH /broadcasts/{id}` — edit, as the creator.
///
/// # Errors
///
/// 403 when the caller is not the creator, 409 when it is live, 422 when the body is
/// unacceptable.
pub async fn update(
    State(handlers): State<Handlers>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<UpdateBroadcastRequest>,
) -> Outcome<super::dto::BroadcastResponse> {
    request.validate()?;
    let actor = summary_of(&auth);

    let broadcast = handlers.service.update(id, request, &actor).await?;
    Ok(MenoResponse::ok("Broadcast updated", broadcast))
}

/// `DELETE /broadcasts/{id}` — soft-delete, as the creator.
///
/// # Errors
///
/// 403 when the caller is not the creator, 409 when it is live.
pub async fn delete(
    State(handlers): State<Handlers>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Outcome<()> {
    let actor = summary_of(&auth);

    handlers.service.delete(id, &actor).await?;
    Ok(MenoResponse::no_content("Broadcast deleted successfully"))
}

/// `POST /broadcasts/{id}/live` — go live.
///
/// # Errors
///
/// 403 when the caller is not the creator, 409 when it is already live, 503 when the
/// media server refuses.
pub async fn go_live(
    State(handlers): State<Handlers>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Outcome<BroadcastSessionResponse> {
    let actor = summary_of(&auth);

    let session = handlers.service.go_live(id, &actor).await?;
    Ok(MenoResponse::created("Broadcast is live", session))
}

/// `POST /broadcasts/{id}/end` — end it.
///
/// # Errors
///
/// 403 when the caller is not the creator, 409 when it is not live.
pub async fn end(
    State(handlers): State<Handlers>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Outcome<EndBroadcastResponse> {
    let actor = summary_of(&auth);

    let ended = handlers.service.end(id, &actor).await?;
    Ok(MenoResponse::ok("Broadcast ended", ended))
}

/// `POST /broadcasts/{id}/join` — join a live broadcast.
///
/// # Errors
///
/// 403 for the creator joining their own, 409 when it is not live or the caller is
/// already in it, 503 when the media server refuses.
pub async fn join(
    State(handlers): State<Handlers>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Outcome<BroadcastSessionResponse> {
    let user = summary_of(&auth);

    let session = handlers.service.join(id, &user).await?;
    Ok(MenoResponse::ok("Broadcast joined", session))
}

/// `POST /broadcasts/{id}/leave`.
///
/// # Errors
///
/// 403 when the caller is not in the broadcast, 409 when the caller is the host.
pub async fn leave(
    State(handlers): State<Handlers>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Outcome<LeaveBroadcastResponse> {
    let user = summary_of(&auth);

    let left = handlers.service.leave(id, &user).await?;
    Ok(MenoResponse::ok("Broadcast left", left))
}

/// `POST /broadcasts/{id}/cohosts` — add one, as the creator.
///
/// # Errors
///
/// 403 when the caller is not the creator or is adding themselves, 404 for an unknown
/// user, 409 when they are already a cohost or the limit is reached.
pub async fn add_cohost(
    State(handlers): State<Handlers>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    axum::Json(request): axum::Json<AddCohostRequest>,
) -> Outcome<super::dto::CohostResponse> {
    let actor = summary_of(&auth);

    let cohost = handlers.service.add_cohost(id, request, &actor).await?;
    Ok(MenoResponse::ok("Cohost added successfully", cohost))
}

/// `DELETE /broadcasts/{id}/cohosts/{cohostId}` — remove one, as the creator.
///
/// The body is optional, so it is read with `Option<Json<…>>` and defaulted: §9.5 wants
/// one documented way to say "also take them out of the room", and an absent body must
/// mean the safe branch.
///
/// # Errors
///
/// 403 when the caller is not the creator, 404 when there is no such cohost.
pub async fn remove_cohost(
    State(handlers): State<Handlers>,
    Extension(auth): Extension<AuthUser>,
    Path((id, cohost_id)): Path<(Uuid, Uuid)>,
    body: Option<axum::Json<RemoveCohostRequest>>,
) -> Outcome<()> {
    let actor = summary_of(&auth);
    let remove_from_room = body.map(|axum::Json(r)| r.remove_from_room).unwrap_or(true);

    handlers
        .service
        .remove_cohost(id, cohost_id, &actor, remove_from_room)
        .await?;
    Ok(MenoResponse::no_content("Cohost removed successfully"))
}

/// `GET /broadcasts/{id}/participants` — the roster.
///
/// # Errors
///
/// 404 for an unknown broadcast, 400 `INVALID_CURSOR` for a bad cursor.
pub async fn participants(
    State(handlers): State<Handlers>,
    Path(id): Path<Uuid>,
    Query(query): Query<ParticipantQuery>,
) -> Outcome<CursorPage<super::dto::ParticipantListItem>> {
    let page = handlers.service.participants(id, &query).await?;
    Ok(MenoResponse::ok("Participants retrieved", page))
}
