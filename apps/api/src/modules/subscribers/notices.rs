//! Where a new follow is announced.
//!
//! # Why a port
//!
//! The follow row and the notification are two facts, and only the first is this
//! module's: [`SubscribersRepo`] can record a follow with no notification module in
//! sight, and a deployment with notifications disabled must still be able to follow
//! somebody (§4.6). [`SubscriberNotices`] is the seam the service writes through; the
//! production adapter in `state.rs` renders an `user_subscribed` row through
//! [`crate::modules::notifications`], and [`SilentNotices`] is the deployment that has
//! no inbox.
//!
//! The actor is a [`SubscriberProfile`] rather than a bare id because the template is
//! `{actor} follows you`: the sentence needs a name, and reading it at the call site is
//! one query the service has already paid for (it needs the follower's existence
//! anyway).

use async_trait::async_trait;
use uuid::Uuid;

use super::repository::SubscriberProfile;

/// Announces that one account started following another.
#[async_trait]
pub trait SubscriberNotices: Send + Sync + 'static {
    /// Tell `target_id` that `actor` follows them.
    ///
    /// Never fails the follow: a follow that was recorded and not announced is a
    /// missing inbox row, while a follow refused because the inbox could not be
    /// written is a broken feature.
    async fn subscribed(&self, actor: &SubscriberProfile, target_id: Uuid);
}

/// A deployment with no notification module.
///
/// Silent rather than refusing, for the same reason [`super::super::broadcast::events::SilentEvents`]
/// is: not being told about a follow is a notification that is not in the inbox, not a
/// reason the follow cannot happen.
#[derive(Debug, Clone, Copy, Default)]
pub struct SilentNotices;

#[async_trait]
impl SubscriberNotices for SilentNotices {
    async fn subscribed(&self, _actor: &SubscriberProfile, _target_id: Uuid) {}
}
