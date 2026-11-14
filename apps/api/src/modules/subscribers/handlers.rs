//! The subscribers endpoints (plan §9.3).
//!
//! Each function is parse → delegate → respond. Nothing here decides anything
//! [`super::service`] can decide; the only `if` a handler is allowed is which extractor it
//! received.
//!
//! # The four list handlers are two rules
//!
//! `get_my_subscribers` and `get_user_subscribers` call the same service method, passing
//! the caller's id twice or the path's id for the owner, and the same is true of the
//! subscriptions pair. Keeping the four handlers is the point: the *routes* differ (one
//! reads `/me`, one reads a path parameter) and a handler is where a route's shape belongs.
//!
//! # The envelope
//!
//! Every function returns [`Outcome`], whose error half is one `MenoError` rendered by the
//! crate's single §4.2 renderer. The two writes answer `no_content` — `200` with no `data`
//! key — which is what `master` sent and what the clients already merge.

use axum::extract::{Extension, Path, Query, State};
use uuid::Uuid;

use super::dto::{FollowDirection, SubscriberDto};
use super::state::SubscribersState;
use crate::middleware::auth::AuthUser;
use crate::types::meno_response::{MenoResponse, Outcome};
use meno_core::{CursorPage, CursorParams};

/// `POST /subscribers/{id}` — follow a user.
///
/// # Errors
///
/// `400` when the id is the caller's own; `404` when no live account carries it.
pub async fn subscribe(
    State(state): State<SubscribersState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Outcome<()> {
    state.service.subscribe(auth.id, id).await?;
    Ok(MenoResponse::no_content("Subscribed to user successfully"))
}

/// `DELETE /subscribers/{id}` — stop following a user.
///
/// # Errors
///
/// `404` when no live account carries the id. Not following them is a success.
pub async fn unsubscribe(
    State(state): State<SubscribersState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Outcome<()> {
    state.service.unsubscribe(auth.id, id).await?;
    Ok(MenoResponse::no_content("Unsubscribed successfully"))
}

/// `GET /subscribers/me/subscribers` — who follows the caller.
///
/// # Errors
///
/// `400` `INVALID_CURSOR` when `?cursor=` cannot be decoded; `404` if the caller's account
/// is gone.
pub async fn get_my_subscribers(
    State(state): State<SubscribersState>,
    Extension(auth): Extension<AuthUser>,
    Query(params): Query<CursorParams>,
) -> Outcome<CursorPage<SubscriberDto>> {
    let page = state
        .service
        .page(FollowDirection::Followers, auth.id, auth.id, &params)
        .await?;
    Ok(MenoResponse::ok(FollowDirection::Followers.label(), page))
}

/// `GET /subscribers/me/subscriptions` — who the caller follows.
///
/// # Errors
///
/// `400` `INVALID_CURSOR` when `?cursor=` cannot be decoded; `404` if the caller's account
/// is gone.
pub async fn get_my_subscriptions(
    State(state): State<SubscribersState>,
    Extension(auth): Extension<AuthUser>,
    Query(params): Query<CursorParams>,
) -> Outcome<CursorPage<SubscriberDto>> {
    let page = state
        .service
        .page(FollowDirection::Following, auth.id, auth.id, &params)
        .await?;
    Ok(MenoResponse::ok(FollowDirection::Following.label(), page))
}

/// `GET /subscribers/{id}/subscribers` — who follows another user.
///
/// # Errors
///
/// `404` when no live account carries the id; `400` `INVALID_CURSOR` for a malformed
/// cursor.
pub async fn get_user_subscribers(
    State(state): State<SubscribersState>,
    Extension(auth): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
    Query(params): Query<CursorParams>,
) -> Outcome<CursorPage<SubscriberDto>> {
    let page = state
        .service
        .page(FollowDirection::Followers, user_id, auth.id, &params)
        .await?;
    Ok(MenoResponse::ok(FollowDirection::Followers.label(), page))
}

/// `GET /subscribers/{id}/subscriptions` — who another user follows.
///
/// # Errors
///
/// `404` when no live account carries the id; `400` `INVALID_CURSOR` for a malformed
/// cursor.
pub async fn get_user_subscriptions(
    State(state): State<SubscribersState>,
    Extension(auth): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
    Query(params): Query<CursorParams>,
) -> Outcome<CursorPage<SubscriberDto>> {
    let page = state
        .service
        .page(FollowDirection::Following, user_id, auth.id, &params)
        .await?;
    Ok(MenoResponse::ok(FollowDirection::Following.label(), page))
}
