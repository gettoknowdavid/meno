//! The notifications endpoints (plan §9.3).
//!
//! Each function is parse → delegate → respond. The caller's identity arrives as
//! `Extension<AuthUser>` and is passed as the owner, so no layer below has to
//! remember which id is whose. Nothing here decides anything [`super::service`] can
//! decide.
//!
//! # The paths, unchanged from `master`
//!
//! `GET /notifications`, `GET /notifications/unread-count`,
//! `PATCH /notifications/{id}/read`, `PATCH /notifications/read-all` and
//! `DELETE /notifications/{id}`. The clients already call these, and the query
//! parameter (`unreadOnly`, plus the shared cursor pair) is the one they already send.

use axum::extract::{Extension, Path, Query, State};
use uuid::Uuid;

use super::dto::{
    MarkAllReadResponse, MarkReadResponse, NotificationItem, NotificationQuery, UnreadCountResponse,
};
use super::state::NotificationsState;
use crate::middleware::auth::AuthUser;
use crate::types::meno_response::{MenoResponse, Outcome};
use meno_core::CursorPage;

/// `GET /notifications` — a page of the caller's inbox.
///
/// # Errors
///
/// `400` `INVALID_CURSOR` when `?cursor=` cannot be decoded.
pub async fn get_notifications(
    State(state): State<NotificationsState>,
    Extension(auth): Extension<AuthUser>,
    Query(query): Query<NotificationQuery>,
) -> Outcome<CursorPage<NotificationItem>> {
    let page = state.service.list(auth.id, &query).await?;
    Ok(MenoResponse::ok("Notifications retrieved", page))
}

/// `GET /notifications/unread-count` — the badge.
///
/// # Errors
///
/// `500` only when the cache is cold and the database read fails.
pub async fn get_unread_count(
    State(state): State<NotificationsState>,
    Extension(auth): Extension<AuthUser>,
) -> Outcome<UnreadCountResponse> {
    let count = state.service.unread_count(auth.id).await?;
    Ok(MenoResponse::ok("Unread count retrieved", count))
}

/// `PATCH /notifications/{id}/read` — mark one notification read.
///
/// # Errors
///
/// `404` when no live notification carries the id for this caller.
pub async fn mark_read(
    State(state): State<NotificationsState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Outcome<MarkReadResponse> {
    let result = state.service.mark_read(id, auth.id).await?;
    Ok(MenoResponse::ok("Notification marked as read", result))
}

/// `PATCH /notifications/read-all` — mark the whole inbox read.
///
/// # Errors
///
/// `500` if the update fails.
pub async fn mark_all_read(
    State(state): State<NotificationsState>,
    Extension(auth): Extension<AuthUser>,
) -> Outcome<MarkAllReadResponse> {
    let result = state.service.mark_all_read(auth.id).await?;
    Ok(MenoResponse::ok("All notifications marked as read", result))
}

/// `DELETE /notifications/{id}` — remove one notification.
///
/// Idempotent: deleting what is not there answers `200`, because the postcondition
/// holds either way.
///
/// # Errors
///
/// `500` if the delete fails.
pub async fn delete_notification(
    State(state): State<NotificationsState>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Outcome<()> {
    state.service.delete(id, auth.id).await?;
    Ok(MenoResponse::no_content("Notification deleted"))
}
