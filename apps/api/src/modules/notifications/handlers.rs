use crate::modules::notifications::dto;
use crate::modules::notifications::error::NotificationError;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::middleware::extractors::MenoQuery;
use crate::shared::pagination::CursorPage;
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::Extension;
use axum::extract::{Path, State};
use std::sync::Arc;
use uuid::Uuid;

/// `GET /notifications`
///
/// Returns the authenticated user's notifications, cursor-paginated.
/// The response envelope also includes `unreadCount` (from Redis).
pub async fn get_notifications(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    MenoQuery(query): MenoQuery<dto::NotificationQuery>,
) -> Result<MenoResponse<CursorPage<dto::NotificationListItem>>, NotificationError> {
    let page = app.notifications.service.list(auth.id, &query).await?;
    Ok(MenoResponse::ok("Notifications retrieved", page))
}

/// `GET /notifications/unread-count`
///
/// Served from Redis — sub-millisecond latency.
pub async fn get_unread_count(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
) -> Result<MenoResponse<dto::UnreadCountResponse>, NotificationError> {
    let count = app.notifications.service.unread_count(auth.id).await?;
    Ok(MenoResponse::ok("Unread count retrieved", count))
}

/// `PATCH /notifications/:id/read`
pub async fn mark_read(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<dto::MarkReadResponse>, NotificationError> {
    let result = app.notifications.service.mark_read(id, auth.id).await?;
    Ok(MenoResponse::ok("Notification marked as read", result))
}

/// `PATCH /notifications/read-all`
pub async fn mark_all_read(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
) -> Result<MenoResponse<dto::MarkAllReadResponse>, NotificationError> {
    let result = app.notifications.service.mark_all_read(auth.id).await?;
    Ok(MenoResponse::ok("All notifications marked as read", result))
}

/// `DELETE /notifications/:id`
pub async fn delete_notification(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<()>, NotificationError> {
    app.notifications.service.delete(id, auth.id).await?;
    Ok(MenoResponse::no_content("Notification deleted"))
}
