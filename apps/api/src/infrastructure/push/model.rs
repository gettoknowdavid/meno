//! The types callers hand to — and read back from — the push adapter.
//!
//! Split from [`super::dto`], which holds FCM's *wire* vocabulary, because these are
//! *ours*. [`dto::FcmEnvelope`] is what Google accepts; [`PushMessage`] is what a
//! notification service means. Keeping them apart is what lets the Android channel id
//! or the APNs payload change without a caller recompiling against FCM's schema.
//!
//! The mirror of `infrastructure::ws`, where `model.rs` holds `WsEvent` and `dto.rs`
//! holds the frames — same split, same reason.

use std::collections::HashMap;

use uuid::Uuid;

use crate::infrastructure::push::error::{PushError, TokenStoreError};

/// `data` key holding the route the app should open when the notification is tapped.
///
/// The constant exists because it is a contract with `apps/mobile`: the Flutter
/// notification handler reads exactly this key. A literal at each end would drift, and
/// the failure is silent — the notification arrives and tapping it does nothing.
pub const DATA_DEEP_LINK: &str = "deep_link";

/// `data` key carrying the recipient's user id.
///
/// Lets the client attribute a push it received to an account without a round trip,
/// which matters when the push arrived while the app was signed out.
pub const DATA_USER_ID: &str = "user_id";

/// A notification to deliver, in terms the caller thinks in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PushMessage {
    /// Notification title.
    pub title: String,

    /// Notification body.
    pub body: String,

    /// Optional image URL shown with the notification.
    pub image: Option<String>,

    /// Key/value payload delivered verbatim to the client.
    ///
    /// FCM requires both keys and values to be strings, which is why this is a
    /// `HashMap<String, String>` and not a serialisable struct: nesting anything deeper
    /// would be rejected by the API, not by us.
    pub data: HashMap<String, String>,
}

impl PushMessage {
    /// A titled notification with no image and an empty payload.
    #[must_use]
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            image: None,
            data: HashMap::new(),
        }
    }

    /// Attach an image.
    #[must_use]
    pub fn with_image(mut self, image: impl Into<String>) -> Self {
        self.image = Some(image.into());
        self
    }

    /// Add one `data` entry.
    #[must_use]
    pub fn with_data(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }

    /// The notification every push in the product is a variation of: a notification
    /// for `user_id` that opens `deep_link` when tapped.
    ///
    /// # Why a constructor and not a builder call at each site
    ///
    /// On `master` the same four lines — insert `deep_link`, insert `user_id` — were
    /// written at each of the notification sites, and the fan-out path inserted
    /// `deep_link` *without* `user_id`. That asymmetry is invisible until a client
    /// tries to attribute a push and cannot. One constructor makes the payload
    /// identical everywhere, and the tests assert it.
    #[must_use]
    pub fn for_user(
        title: impl Into<String>,
        body: impl Into<String>,
        user_id: Uuid,
        deep_link: &str,
    ) -> Self {
        Self::new(title, body)
            .with_data(DATA_DEEP_LINK, deep_link)
            .with_data(DATA_USER_ID, user_id.to_string())
    }

    /// The deep link this notification opens, if it carries one.
    #[must_use]
    pub fn deep_link(&self) -> Option<&str> {
        self.data.get(DATA_DEEP_LINK).map(String::as_str)
    }
}

/// One device to deliver to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushTarget {
    /// The user this token belongs to.
    ///
    /// Carried alongside the token so a failed multicast can say *which* users need
    /// their tokens cleaned up without the caller keeping a parallel map in sync.
    pub user_id: Uuid,

    /// The device's FCM registration token.
    pub device_token: String,
}

impl PushTarget {
    /// A target for `user_id`'s device.
    #[must_use]
    pub fn new(user_id: Uuid, device_token: impl Into<String>) -> Self {
        Self {
            user_id,
            device_token: device_token.into(),
        }
    }
}

/// The outcome of a fan-out.
///
/// Per-device failures are collected rather than propagated: one stale token in a
/// 500-strong subscriber list must not abort the other 499 notifications, and the
/// caller needs the list of stale tokens to clean up afterwards.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MulticastResult {
    /// `(user_id, error)` for every device that failed.
    pub failed: Vec<(Uuid, PushError)>,

    /// How many devices were reached.
    pub succeeded: usize,
}

impl MulticastResult {
    /// The users whose tokens are no longer registered and should be deleted.
    ///
    /// The reason this adapter bothers to tag every failure with a `user_id`:
    /// without it the caller has to re-derive which token in its input list failed,
    /// which is the kind of index arithmetic that is right until it is not.
    #[must_use]
    pub fn stale_tokens(&self) -> Vec<Uuid> {
        self.failed
            .iter()
            .filter(|(_, error)| error.is_stale_token())
            .map(|(user_id, _)| *user_id)
            .collect()
    }

    /// How many devices were targeted in total.
    #[must_use]
    pub fn total(&self) -> usize {
        self.succeeded + self.failed.len()
    }

    /// Whether every device was reached.
    #[must_use]
    pub fn is_complete_success(&self) -> bool {
        self.failed.is_empty()
    }

    /// Fold these two results together, for a caller fanning out in batches.
    pub fn merge(&mut self, other: Self) {
        self.succeeded += other.succeeded;
        self.failed.extend(other.failed);
    }
}

/// What happened when a push was attempted for one user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotifyOutcome {
    /// The notification reached the device.
    Delivered,

    /// Nothing was sent, and nothing should have been: push is disabled, or the user
    /// has no usable device token.
    ///
    /// Its own variant rather than `Delivered`-with-a-note because a notification that
    /// was *supposed* to go out and did not is a bug, and the two must be
    /// distinguishable in a metric.
    Skipped,

    /// The send failed for a reason no retry will fix on its own.
    Failed(PushError),

    /// The device token could not be read at all, so nothing was attempted.
    ///
    /// Distinct from [`Self::Skipped`]: there is no device token because the *lookup*
    /// failed, not because the user has none. Reporting it as `Skipped` would make a
    /// database outage look like a quiet user.
    LookupFailed(TokenStoreError),

    /// FCM rejected the token as unregistered, and it has been deleted.
    TokenCleared,
}

impl NotifyOutcome {
    /// Whether a device actually received the notification.
    #[must_use]
    pub const fn is_delivered(&self) -> bool {
        matches!(self, Self::Delivered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn for_user_carries_both_the_route_and_the_recipient() {
        // The whole point of the constructor: identical payloads everywhere. A caller
        // that hand-rolled its `data` map and forgot `user_id` produced a notification
        // the client could not attribute.
        let user = id(7);
        let message = PushMessage::for_user("Ada", "Live now", user, "/broadcasts/1");

        assert_eq!(
            message.data.get(DATA_DEEP_LINK).map(String::as_str),
            Some("/broadcasts/1")
        );
        assert_eq!(
            message.data.get(DATA_USER_ID).map(String::as_str),
            Some(user.to_string().as_str())
        );
        assert_eq!(message.deep_link(), Some("/broadcasts/1"));
        assert_eq!(message.title, "Ada");
        assert_eq!(message.body, "Live now");
    }

    #[test]
    fn a_message_without_a_deep_link_says_so() {
        // `deep_link()` returning `None` must be distinguishable from an empty string,
        // which would open the app's home screen.
        assert_eq!(PushMessage::new("Ada", "Live now").deep_link(), None);
        assert_eq!(
            PushMessage::new("Ada", "Live now")
                .with_data(DATA_DEEP_LINK, "")
                .deep_link(),
            Some("")
        );
    }

    #[test]
    fn the_builder_methods_do_not_mutate_the_original() {
        // `with_*` returns a new value; a `&mut self` chain would make the shared
        // `PushMessage` a fan-out footgun, since every recipient would inherit the last
        // recipient's deep link.
        let base = PushMessage::new("Ada", "Live now");
        let with_image = base.clone().with_image("https://cdn.example.com/a.png");

        assert_eq!(base.image, None);
        assert_eq!(
            with_image.image.as_deref(),
            Some("https://cdn.example.com/a.png")
        );
    }

    #[test]
    fn only_stale_tokens_are_offered_for_cleanup() {
        // Everything else in `failed` is a transient or upstream problem: deleting
        // those tokens would log every user out of push because FCM had a bad minute.
        let result = MulticastResult {
            succeeded: 1,
            failed: vec![
                (id(1), PushError::TokenInvalid),
                (id(2), PushError::RateLimited),
                (id(3), PushError::SendFailed { status: 500 }),
                (id(4), PushError::TokenInvalid),
            ],
        };

        assert_eq!(result.stale_tokens(), vec![id(1), id(4)]);
        assert_eq!(result.total(), 5);
        assert!(!result.is_complete_success());
    }

    #[test]
    fn an_all_successful_fan_out_offers_nothing_and_says_it_succeeded() {
        let result = MulticastResult {
            succeeded: 3,
            failed: Vec::new(),
        };

        assert!(result.stale_tokens().is_empty());
        assert!(result.is_complete_success());
        assert_eq!(result.total(), 3);
    }

    #[test]
    fn merging_batches_preserves_the_stale_token_list() {
        // Fan-out is batched, so `merge` is on the critical path: if it dropped the
        // failures, no token would ever be cleaned up.
        let mut first = MulticastResult {
            succeeded: 2,
            failed: vec![(id(1), PushError::TokenInvalid)],
        };
        first.merge(MulticastResult {
            succeeded: 1,
            failed: vec![(id(2), PushError::CircuitOpen)],
        });

        assert_eq!(first.succeeded, 3);
        assert_eq!(first.total(), 5);
        assert_eq!(first.stale_tokens(), vec![id(1)]);
    }

    #[test]
    fn an_empty_fan_out_is_a_complete_success() {
        // A broadcast with no subscribers is not an error; reporting it as a partial
        // failure would page someone for an audience of nobody.
        let result = MulticastResult::default();

        assert_eq!(result.total(), 0);
        assert!(result.is_complete_success());
        assert!(result.stale_tokens().is_empty());
    }

    #[test]
    fn only_delivered_counts_as_delivered() {
        // `TokenCleared` and `Delivered` both mean "something happened", and conflating
        // them would make a notification-count metric quietly wrong.
        assert!(NotifyOutcome::Delivered.is_delivered());
        assert!(!NotifyOutcome::Skipped.is_delivered());
        assert!(!NotifyOutcome::TokenCleared.is_delivered());
        assert!(!NotifyOutcome::Failed(PushError::CircuitOpen).is_delivered());
        assert!(
            !NotifyOutcome::LookupFailed(TokenStoreError::Lookup("db".to_owned())).is_delivered()
        );
    }

    #[test]
    fn a_target_keeps_its_owner() {
        let target = PushTarget::new(id(3), "device-token");
        assert_eq!(target.user_id, id(3));
        assert_eq!(target.device_token, "device-token");
    }
}
