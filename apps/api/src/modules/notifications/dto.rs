//! The notifications wire contract (§9.5).
//!
//! # Casing and dates
//!
//! `camelCase`, like every other refactored module: `typeCode`, `imageUrl`,
//! `broadcastId`, `deepLink`, `createdAt`. `master` emitted `type_code`,
//! `image_url`, `broadcast_id` and `deep_link` for the same fields, so a client
//! needed a second naming rule for this one feature. Every timestamp is RFC 3339
//! with `#[serde(with = "time::serde::rfc3339")]`.
//!
//! # The fields the client actually branches on
//!
//! [`NotificationItem::type_code`] selects the icon and call to action, and
//! [`NotificationItem::deep_link`] is where a tap goes. Both are computed
//! server-side so two clients cannot disagree about either.

use crate::modules::notifications::model::ActorSummary;
use meno_core::CursorParams;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// One notification, rendered.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationItem {
    /// Primary key.
    pub id: Uuid,
    /// The type code — e.g. `"live_broadcast_started"`. Clients switch on this.
    pub type_code: String,
    /// The rendered title, with every placeholder substituted.
    pub title: String,
    /// The rendered body.
    pub body: String,
    /// Actor avatar or broadcast cover, depending on the type.
    pub image_url: Option<String>,
    /// Whether the recipient has read it.
    pub read: bool,
    /// The broadcast the notification refers to, when it refers to one.
    pub broadcast_id: Option<Uuid>,
    /// Who triggered it. `None` for system notifications.
    pub actor: Option<ActorSummary>,
    /// When it was created.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// The deep link a tap opens — `meno://broadcasts/{id}`,
    /// `meno://profile/{id}`, or `meno://home`.
    pub deep_link: String,
}

/// Returned by `GET /notifications/unread-count`.
///
/// The fast path is the Redis counter; the database is the fallback when the key is
/// missing, so the number is never wrong after a restart, only slower once.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnreadCountResponse {
    /// How many unread notifications the caller has.
    pub count: i64,
}

/// Returned by `PATCH /notifications/{id}/read`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkReadResponse {
    /// The notification's id.
    pub id: Uuid,
    /// Always `true`: the postcondition the caller asked for.
    pub read: bool,
}

/// Returned by `PATCH /notifications/read-all`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkAllReadResponse {
    /// How many notifications were marked as read.
    pub updated: u64,
}

/// The query string of `GET /notifications`.
#[derive(Debug, Default, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NotificationQuery {
    /// When `true`, only unread notifications are returned.
    #[serde(default)]
    pub unread_only: bool,

    /// The cursor and page size, shared with every other list endpoint.
    #[serde(flatten)]
    pub pagination: CursorParams,
}

impl NotificationQuery {
    /// The validated page size.
    #[must_use]
    pub fn limit(&self) -> i64 {
        self.pagination.limit()
    }

    /// The page size plus the one probe row.
    #[must_use]
    pub fn limit_plus_one(&self) -> i64 {
        self.pagination.limit_plus_one()
    }

    /// The exclusive keyset boundary, if this is not the first page.
    #[must_use]
    pub fn cursor(&self) -> Option<&meno_core::Cursor> {
        self.pagination.cursor.as_ref()
    }
}

#[cfg(test)]
mod tests {
    //! The wire shape, asserted key by key.

    use super::*;

    fn item() -> NotificationItem {
        NotificationItem {
            id: Uuid::from_u128(1),
            type_code: "live_broadcast_started".to_owned(),
            title: "Live Now".to_owned(),
            body: "Ada is live: Morning Notes".to_owned(),
            image_url: None,
            read: false,
            broadcast_id: Some(Uuid::from_u128(2)),
            actor: None,
            created_at: OffsetDateTime::from_unix_timestamp(1_700_000_000)
                .expect("a plausible instant"),
            deep_link: "meno://broadcasts/2".to_owned(),
        }
    }

    #[test]
    fn a_notification_is_camel_case_with_an_rfc3339_date() {
        let json = serde_json::to_value(item()).expect("serialises");

        for key in [
            "typeCode",
            "imageUrl",
            "broadcastId",
            "createdAt",
            "deepLink",
        ] {
            assert!(json.get(key).is_some(), "missing {key}: {json}");
        }
        assert!(
            json["createdAt"].is_string(),
            "an RFC 3339 string, not a tuple: {json}"
        );
        assert!(
            !json.to_string().contains("type_code"),
            "a snake_case key leaked: {json}"
        );
    }

    #[test]
    fn the_query_reads_unread_only_from_camel_case() {
        let query: NotificationQuery =
            serde_json::from_str(r#"{"unreadOnly":true,"limit":"5"}"#).expect("decodes");

        assert!(query.unread_only);
        assert_eq!(query.limit(), 5);
        assert_eq!(query.limit_plus_one(), 6);
    }

    #[test]
    fn the_query_defaults_to_the_whole_inbox() {
        let query: NotificationQuery = serde_json::from_str("{}").expect("decodes");

        assert!(!query.unread_only);
        assert_eq!(query.limit(), 20);
        assert!(query.cursor().is_none());
    }
}
