//! The profile endpoints (plan §9.3).
//!
//! # Thin on purpose
//!
//! Each function is parse → validate → delegate → respond, the same rule as
//! [`crate::modules::auth::handlers`] and [`crate::modules::broadcast::handlers`]:
//! the rules these endpoints stand for — what a valid bio is, whether a page of
//! search has a next cursor, whether those bytes are a PNG — are testable without
//! HTTP in `service`/`dto`, and a handler that grows an `if` that is not "which
//! extractor did I get" has stolen a decision from them.
//!
//! # The envelope
//!
//! Every function returns `Outcome<T>` — the success envelope or one
//! [`meno_core::Error`] rendered by the crate's single §4.2 renderer. The auth guard
//! that makes `Extension<AuthUser>` always present is applied in
//! [`crate::routes::profile`], never here.
//!
//! # Dates
//!
//! Nothing here formats a timestamp. Every date on these endpoints is written by
//! [`super::dto`] with `#[serde(with = "time::serde::rfc3339")]`, and
//! `apps/api/tests/wire_dates.rs` enforces it.

use axum::extract::{Extension, Path, Query, State};
use uuid::Uuid;

use super::dto::{
    MeResponse, ProfileSearchQuery, ProfileSearchResult, PublicProfileResponse,
    UpdateProfileRequest, UploadAvatarForm, UploadAvatarResponse, Validatable,
};
use super::error;
use super::state::ProfileState;
use crate::middleware::auth::AuthUser;
use crate::middleware::extractors::{MenoBody, MenoMultipartForm};
use crate::types::meno_response::{MenoResponse, Outcome};
use meno_core::CursorPage;

/// `GET /users/me` — the caller's own record, settings embedded.
pub async fn get_me(
    State(state): State<ProfileState>,
    Extension(auth): Extension<AuthUser>,
) -> Outcome<MeResponse> {
    let me = state.service.get_me(auth.id).await?;
    Ok(MenoResponse::ok("Profile retrieved", me))
}

/// `PATCH /users/me` — a partial update; the response is the fresh record.
pub async fn update_me(
    State(state): State<ProfileState>,
    Extension(auth): Extension<AuthUser>,
    MenoBody(body): MenoBody<UpdateProfileRequest>,
) -> Outcome<MeResponse> {
    body.validate()?;
    let me = state.service.update_me(auth.id, &body).await?;
    Ok(MenoResponse::ok("Profile updated", me))
}

/// `GET /users/{id}` — someone else's public profile.
pub async fn get_profile(
    State(state): State<ProfileState>,
    Extension(auth): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
) -> Outcome<PublicProfileResponse> {
    let profile = state.service.get_profile(auth.id, user_id).await?;
    Ok(MenoResponse::ok("Profile retrieved", profile))
}

/// `GET /users?q=&limit=&cursor=` — one keyset-paginated page of search results.
pub async fn search(
    State(state): State<ProfileState>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<ProfileSearchQuery>,
) -> Outcome<CursorPage<ProfileSearchResult>> {
    query.validate()?;
    let page = state.service.search(auth.id, &query).await?;
    Ok(MenoResponse::ok("Profiles retrieved", page))
}

/// `POST /users/me/avatar` — one-step multipart upload (§3.3).
///
/// The multipart extractor has already separated text fields from files, so this
/// handler's only judgement is *which* part counts as the avatar — `file`, by name —
/// and then handing the bytes to the service, which is where "is it an image" lives.
pub async fn upload_avatar(
    State(state): State<ProfileState>,
    Extension(auth): Extension<AuthUser>,
    form: MenoMultipartForm<UploadAvatarForm>,
) -> Outcome<UploadAvatarResponse> {
    let file = form
        .files
        .get("file")
        .ok_or_else(error::avatar_part_missing)?;
    if file.is_empty() {
        return Err(error::avatar_empty().into());
    }

    let uploaded = state.service.upload_avatar(auth.id, &file.bytes).await?;
    Ok(MenoResponse::ok("Avatar uploaded", uploaded))
}
