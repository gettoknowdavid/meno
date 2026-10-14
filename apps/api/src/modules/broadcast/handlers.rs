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
    UpdateBroadcastRequest, UploadBroadcastImageForm, Validatable,
};
use super::error;
use super::service::{BroadcastService, summary_of};
use super::storage::BroadcastStorage;
use crate::middleware::auth::AuthUser;
use crate::middleware::extractors::MenoMultipartForm;
use crate::types::meno_response::{MenoResponse, Outcome};
use meno_core::CursorPage;

/// The broadcast module's rules, as a handler sees them.
///
/// Two collaborators rather than one because the cover upload needs both and they answer
/// different questions: the service decides *whether* a caller may replace a cover and
/// keeps the row in step ([`BroadcastService::set_image`]), and the handler needs the ids
/// it was handed to name the object that flow already wrote. Nothing here holds a
/// concrete adapter — both are the module's own types, built in `state.rs`.
#[derive(Clone)]
pub struct Handlers {
    service: BroadcastService,
    storage: BroadcastStorage,
}

impl Handlers {
    /// Project the module's service and store for the routing layer.
    #[must_use]
    pub const fn new(service: BroadcastService, storage: BroadcastStorage) -> Self {
        Self { service, storage }
    }
}

impl std::fmt::Debug for Handlers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handlers")
            .field("service", &self.service)
            .field("storage", &self.storage)
            .finish()
    }
}

impl From<BroadcastService> for Handlers {
    /// A service on its own, with no object store.
    ///
    /// Kept because a `Handlers` built this way is exactly a deployment with
    /// `STORAGE_ENABLED` unset: every route works and the cover upload answers 503 — the
    /// §4.6 behaviour, not a broken state.
    fn from(service: BroadcastService) -> Self {
        Self::new(service, BroadcastStorage::disabled())
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

/// `POST /broadcasts/{id}/image` — upload the cover in one call (§3.3).
///
/// The multipart extractor has already separated text fields from files, so this
/// handler's only judgement is *which* part counts as the image — `file`, by name — and
/// then handing the bytes to [`BroadcastService::set_image`], which is where "is it an
/// image" and "may this caller replace it" live. The response is the updated broadcast,
/// so the client that just uploaded can drop it straight into its state rather than
/// re-fetching the row it already has.
///
/// # Errors
///
/// 422 with field `file` when the part is missing, empty, oversized or not an image; 403
/// when the caller is not the creator; 409 when the broadcast is live; 503 when the
/// object store refuses.
pub async fn upload_image(
    State(handlers): State<Handlers>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    form: MenoMultipartForm<UploadBroadcastImageForm>,
) -> Outcome<super::dto::BroadcastResponse> {
    let file = form
        .files
        .get("file")
        .ok_or_else(error::image_part_missing)?;
    if file.is_empty() {
        return Err(error::image_empty().into());
    }

    let actor = summary_of(&auth);

    let broadcast = handlers.service.set_image(id, &file.bytes, &actor).await?;
    Ok(MenoResponse::ok("Broadcast image uploaded", broadcast))
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
