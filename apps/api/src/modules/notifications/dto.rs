use crate::shared::pagination::CursorParams;
use crate::shared::types::dto::UserSummary;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::{OffsetDateTime, serde::rfc3339};
use uuid::Uuid;

/// The shape returned for every notification in list responses.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationListItem {
    pub id: Uuid,

    /// The notification type code (e.g. "live_broadcast_started").
    /// Flutter switches on this to decide which icon / CTA to render.
    pub type_code: String,

    pub title: String,
    pub body: String,

    /// Actor avatar URL or broadcast cover image, depending on type.
    pub image_url: Option<String>,

    pub read: bool,

    pub broadcast_id: Option<Uuid>,

    /// The user who triggered this notification (sender, creator, subscriber).
    /// `None` for system-generated notifications.
    pub actor: Option<UserSummary>,

    #[serde(with = "rfc3339")]
    pub created_at: OffsetDateTime,

    /// Deep link the client navigates to on tap.
    /// e.g. "meno://broadcasts/{id}" or "meno://profile/{id}"
    pub deep_link: String,
}

/// Returned by `GET /notifications/unread-count`.
/// Served from Redis for O(1) latency.
#[derive(Debug, Serialize)]
pub struct UnreadCountResponse {
    pub count: i64,
}

/// Returned by `PATCH /notifications/:id/read`.
#[derive(Debug, Serialize)]
pub struct MarkReadResponse {
    pub id: Uuid,
    pub read: bool,
}

/// Returned by `PATCH /notifications/read-all`.
#[derive(Debug, Serialize)]
pub struct MarkAllReadResponse {
    /// Number of notifications that were marked as read.
    pub updated: u64,
}

#[derive(Debug, Default, Deserialize, Clone)]
pub struct NotificationQuery {
    /// When `true`, only unread notifications are returned.
    #[serde(default)]
    pub unread_only: bool,

    #[serde(flatten)]
    pub pagination: CursorParams,
}
impl NotificationQuery {
    pub fn limit(&self) -> i64 {
        self.pagination.limit()
    }
    pub fn limit_plus_one(&self) -> i64 {
        self.pagination.limit_plus_one()
    }
    pub fn cursor(&self) -> Option<&crate::shared::pagination::Cursor> {
        self.pagination.cursor.as_ref()
    }
}
