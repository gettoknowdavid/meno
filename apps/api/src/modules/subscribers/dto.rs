//! The subscribers wire contract (§9.5).
//!
//! # Casing and dates
//!
//! `camelCase`, like `broadcast`, `chat`, `notes` and `profile`: `fullName`, `avatarUrl`,
//! `isFollowing`, `subscribedAt`. `master` emitted `snake_case` for the same fields, which
//! meant one feature with a second naming rule. Every timestamp is RFC 3339 with
//! `#[serde(with = "time::serde::rfc3339")]`, so a date cannot become a nine-number tuple
//! by accident; `apps/api/tests/wire_dates.rs` enforces that for this module like every
//! other.
//!
//! # One type, because the row is the response
//!
//! [`SubscriberDto`] is both the row `repository::pg` selects and what the wire carries.
//! That is `profile`'s `ProfileSearchResult` decision, made again here for the same reason:
//! a follower row is a user's public fields plus one viewer-relative flag, and `email` is
//! never selected, so there is nothing a separate row type would keep off the wire.
//!
//! # The one field whose name predates the refactor
//!
//! `subscribedAt` is the `user_subscribers` row's `created_at` — when the follow started —
//! and it is the sort key of *both* lists. `master` called it `subscribedAt` in the
//! followers list and reused the name in the following list, where it means "when the
//! viewer started following them". The name is kept because the clients read it; the
//! meaning is the same timestamp either way.

use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

/// Which of the two collections a request is asking for.
///
/// One enum rather than two pairs of near-identical methods: the two lists are the same
/// query with the owner and the counterpart column swapped (`subscription_id` is the
/// owner of a followers list, `subscriber_id` is the owner of a following list), and the
/// repository turns this into that pair. `master` spelled both statements out in full and
/// they differed by two tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowDirection {
    /// People who follow the owner — `GET …/subscribers`.
    Followers,
    /// People the owner follows — `GET …/subscriptions`.
    Following,
}

impl FollowDirection {
    /// The label a page of this direction is answered with.
    ///
    /// Used for the envelope message, so the two lists cannot describe themselves with
    /// each other's word.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Followers => "Subscribers retrieved",
            Self::Following => "Subscriptions retrieved",
        }
    }
}

/// A user as a follow list carries them.
///
/// The `id` is the *other* user's: the follower's in a followers list, the followee's in a
/// following list. `is_following` answers for the viewer, so the same row read by two
/// different callers is two different answers.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SubscriberDto {
    /// The user's id.
    pub id: Uuid,
    /// The display name.
    pub full_name: String,
    /// The bio, if any.
    pub bio: Option<String>,
    /// CDN URL of the avatar, if one was uploaded.
    pub avatar_url: Option<String>,
    /// Whether the *viewer* follows this user.
    pub is_following: bool,
    /// When the follow started — the keyset sort key of both lists.
    #[serde(with = "time::serde::rfc3339")]
    pub subscribed_at: OffsetDateTime,
}

#[cfg(test)]
mod tests {
    //! The wire shape, asserted key by key.

    use super::*;

    fn instant() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("a plausible instant")
    }

    fn subscriber() -> SubscriberDto {
        SubscriberDto {
            id: Uuid::from_u128(1),
            full_name: "Grace Hopper".to_owned(),
            bio: Some("compilers".to_owned()),
            avatar_url: Some("https://cdn.example/avatars/1.png".to_owned()),
            is_following: true,
            subscribed_at: instant(),
        }
    }

    #[test]
    fn a_subscriber_row_is_camel_case_with_an_rfc3339_date() {
        let json = serde_json::to_value(subscriber()).expect("serialises");

        assert!(json.get("fullName").is_some(), "{json}");
        assert!(json.get("avatarUrl").is_some(), "{json}");
        assert!(json.get("isFollowing").is_some(), "{json}");
        assert!(json.get("subscribedAt").is_some(), "{json}");
        assert!(
            json["subscribedAt"].is_string(),
            "an RFC 3339 string, not a tuple: {json}"
        );
        assert!(
            !json.to_string().contains("full_name"),
            "a snake_case key leaked: {json}"
        );
    }

    #[test]
    fn the_two_directions_describe_themselves_differently() {
        assert_eq!(FollowDirection::Followers.label(), "Subscribers retrieved");
        assert_eq!(
            FollowDirection::Following.label(),
            "Subscriptions retrieved"
        );
    }
}
