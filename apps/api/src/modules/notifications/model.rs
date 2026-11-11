//! The notifications module's rows, and the type codes the system speaks.
//!
//! # What changed from `master`
//!
//! - **The row carries a template, not a rendered sentence.** `master` stored the
//!   template *and* rendered it in two places; here
//!   [`NotificationRow`] is the flat row the page query returns — template text,
//!   actor projection and broadcast title — and the service turns it into one
//!   [`super::dto::NotificationItem`]. The intermediate type is private to the
//!   storage adapter's mapping, not part of the module's stored shape.
//! - **The actor is this module's own projection.** `master` used
//!   `shared::types::dto::UserSummary`, which no longer exists: the layer rule keeps
//!   one module from reaching into another, and `chat` solved the same problem the
//!   same way with its own `SenderSummary`.
//! - **The codes are the migration's seed, written once.** [`codes`] names the five
//!   `notification_types` rows migration `0012` inserts, so a template lookup and a
//!   deep link cannot disagree about how a type is spelled.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// The template variable for the broadcast's title.
///
/// `{title}` and `{broadcast}` are both substituted from it, because `master`'s
/// seeded templates use both spellings for the same fact.
pub const BROADCAST_TITLE_VAR: &str = "title";

/// The template variable for the actor's display name.
pub const ACTOR_VAR: &str = "actor";

/// Every notification type the migrations seed.
///
/// Keep in sync with `crates/db/migrations/0012_notifications_schema.sql`. A code
/// that is not in `notification_types` cannot have a template, and the foreign key
/// in `notification_templates` would refuse one.
pub mod codes {
    /// Somebody was invited to co-host a broadcast.
    pub const ADDED_AS_COHOST: &str = "added_as_cohost";
    /// Somebody started following the recipient.
    pub const USER_SUBSCRIBED: &str = "user_subscribed";
    /// A creator the recipient follows scheduled a broadcast.
    pub const SCHEDULED_BROADCAST: &str = "scheduled_broadcast";
    /// A creator the recipient follows went live.
    pub const LIVE_BROADCAST_STARTED: &str = "live_broadcast_started";
    /// A broadcast the recipient was in has ended.
    pub const BROADCAST_ENDED: &str = "broadcast_ended";
}

/// A template a notification row is rendered from.
///
/// Only the columns rendering needs. `metadata` exists on the table and is
/// deliberately not selected: it is an operator's note, not part of a sentence.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct NotificationTemplate {
    /// Primary key.
    pub id: Uuid,
    /// The type code — the `notification_templates.type` column.
    #[sqlx(rename = "type")]
    pub type_code: String,
    /// The title, with `{actor}` / `{title}` placeholders.
    pub title: String,
    /// The body, with the same placeholders.
    pub body: String,
    /// An image to show with the notification, if the template names one.
    pub image_url: Option<String>,
}

/// Who triggered a notification, as this module carries them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorSummary {
    /// Account id.
    pub id: Uuid,
    /// Display name, substituted for `{actor}`.
    pub full_name: String,
    /// Optional biography.
    pub bio: Option<String>,
    /// Avatar storage key, when one was uploaded.
    pub avatar_id: Option<String>,
    /// Resolved avatar URL, when the deployment publishes one.
    pub avatar_url: Option<String>,
}

/// One row of the inbox as the page query returns it, before rendering.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct NotificationRow {
    /// Primary key.
    pub id: Uuid,
    /// Whether the recipient has read it.
    pub read: bool,
    /// When it was created — the keyset sort key.
    pub created_at: OffsetDateTime,
    /// The template's type code.
    pub type_code: String,
    /// The template's title, with placeholders intact.
    pub title_template: String,
    /// The template's body, with placeholders intact.
    pub body_template: String,
    /// The template's image, with placeholders intact.
    pub image_url_template: Option<String>,
    /// The broadcast the notification refers to, if any.
    pub broadcast_id: Option<Uuid>,
    /// That broadcast's title, for the `{title}` variable.
    pub broadcast_title: Option<String>,
    /// The actor, if the notification has one.
    pub actor_id: Option<Uuid>,
    /// The actor's display name (only when the actor is a live account).
    pub actor_name: Option<String>,
    /// The actor's biography.
    pub actor_bio: Option<String>,
    /// The actor's avatar URL.
    pub actor_avatar_url: Option<String>,
    /// The actor's avatar storage key.
    pub actor_avatar_id: Option<String>,
}

impl NotificationRow {
    /// The actor this row carries, assembled from the join.
    ///
    /// `None` for a system notification, and also for one whose actor has since
    /// been deleted: the join is `LEFT JOIN … AND deleted_at IS NULL`, so a gone
    /// account reads as "no actor" rather than as an empty name.
    #[must_use]
    pub fn actor(&self) -> Option<ActorSummary> {
        // The name, not the id, decides presence: the join keeps `n.actor_id`
        // but drops the user columns for a deleted actor, so an id-only check
        // would render a gone account as an actor with an empty name.
        let full_name = self.actor_name.clone()?;
        let id = self.actor_id?;

        Some(ActorSummary {
            id,
            full_name,
            bio: self.actor_bio.clone(),
            avatar_id: self.actor_avatar_id.clone(),
            avatar_url: self.actor_avatar_url.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    //! The row-to-actor mapping, which is the only logic in this file.

    use super::*;

    fn row(actor_id: Option<Uuid>) -> NotificationRow {
        NotificationRow {
            id: Uuid::from_u128(1),
            read: false,
            created_at: OffsetDateTime::from_unix_timestamp(1_700_000_000)
                .expect("a plausible instant"),
            type_code: codes::USER_SUBSCRIBED.to_owned(),
            title_template: "New Follower".to_owned(),
            body_template: "{actor} started following you".to_owned(),
            image_url_template: None,
            broadcast_id: None,
            broadcast_title: None,
            actor_id,
            actor_name: Some("Ada Lovelace".to_owned()),
            actor_bio: None,
            actor_avatar_url: None,
            actor_avatar_id: None,
        }
    }

    #[test]
    fn a_row_with_an_actor_carries_their_name() {
        let actor = row(Some(Uuid::from_u128(2)))
            .actor()
            .expect("an actor is present");

        assert_eq!(actor.full_name, "Ada Lovelace");
    }

    #[test]
    fn a_row_without_an_actor_stays_without_one() {
        assert!(row(None).actor().is_none());
    }
}
