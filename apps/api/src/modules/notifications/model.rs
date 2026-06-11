use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Notification {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub template_id: Uuid,
    pub actor_id: Option<Uuid>,
    pub broadcast_id: Option<Uuid>,
    pub entity_type: Option<String>,
    pub entity_id: Option<Uuid>,
    pub read: bool,
    pub read_at: Option<OffsetDateTime>,
    pub archived_at: Option<OffsetDateTime>,
    pub custom_metadata: Option<serde_json::Value>,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationTemplate {
    pub id: Uuid,
    pub r#type: String,
    pub title: String,
    pub body: String,
    pub image_url: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub is_active: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationType {
    pub code: String,
    pub label: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub created_at: OffsetDateTime,
}

/// Flat row returned by the notification query that joins templates and actor.
/// The service layer assembles this into `NotificationListItem`.
#[derive(Debug, sqlx::FromRow)]
pub struct NotificationViewRow {
    pub id: Uuid,
    pub read: bool,
    pub created_at: OffsetDateTime,
    pub type_code: String,
    pub title_template: String,
    pub body_template: String,
    pub image_url_template: Option<String>,
    pub actor_id: Option<Uuid>,
    pub actor_name: Option<String>,
    pub actor_bio: Option<String>,
    pub actor_avatar_url: Option<String>,
    pub actor_avatar_id: Option<String>,
    pub broadcast_id: Option<Uuid>,
}

/// Notification type codes used throughout the system.
/// Keep in sync with the `notification_types` DB seed.
pub mod codes {
    pub const ADDED_AS_COHOST: &str = "added_as_cohost";
    pub const USER_SUBSCRIBED: &str = "user_subscribed";
    pub const SCHEDULED_BROADCAST: &str = "scheduled_broadcast";
    pub const LIVE_BROADCAST_STARTED: &str = "live_broadcast_started";
    pub const BROADCAST_ENDED: &str = "broadcast_ended";
}