//! Where a broadcast's lifecycle events reach the people in it (§3.1).
//!
//! # Why this is a port rather than a call to the bridge
//!
//! The same reason [`crate::modules::chat::publisher`] is one: the reaper's invariant is
//! *"a broadcast that was ended was announced to the people in it"*, and calling
//! [`WsPubSubBridge`] directly would make that invariant unassertable without a live
//! Redis. [`BroadcastEvents`] is the seam; production wires [`PubSubEvents`], the tests
//! wire [`recording::RecordingEvents`] and read back who was told.
//!
//! # One delivery per recipient
//!
//! [`BroadcastEvents::ended`] publishes to the listed accounts, not to the room channel,
//! because the list is already exhaustive: the creator, every cohost, and everybody the
//! room server reports as connected. Publishing to the room as well would deliver the
//! same frame twice to anyone in both sets, and a client that reacted twice to
//! `endedBroadcast` would be reacting to this module's convenience.
//!
//! Delivery is best-effort. A client that misses the frame reads the broadcast back and
//! finds it ended, which is why nothing here fails a sweep over one undelivered event.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use super::model::EndReason;
use crate::infrastructure::ws::dto::WsPayload;
use crate::infrastructure::ws::pubsub::WsPubSubBridge;

/// Announces a broadcast's lifecycle events to the accounts concerned.
#[async_trait]
pub trait BroadcastEvents: Send + Sync + 'static {
    /// Tell [recipients] that `broadcast_id` ended, and why.
    async fn ended(&self, broadcast_id: Uuid, reason: EndReason, recipients: &[Uuid]);
}

/// The production publisher: the cross-replica pub/sub bridge.
///
/// Not the local socket registry: a room's participants are spread across replicas, and
/// a phone that reconnected to another instance is exactly the client that must not miss
/// that the broadcast is over.
#[derive(Clone)]
pub struct PubSubEvents {
    bridge: Arc<WsPubSubBridge>,
}

impl PubSubEvents {
    /// Wrap the application's bridge.
    #[must_use]
    pub fn new(bridge: Arc<WsPubSubBridge>) -> Self {
        Self { bridge }
    }
}

impl std::fmt::Debug for PubSubEvents {
    /// Names the field and stops: the bridge holds two Redis clients.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PubSubEvents").finish_non_exhaustive()
    }
}

#[async_trait]
impl BroadcastEvents for PubSubEvents {
    async fn ended(&self, broadcast_id: Uuid, reason: EndReason, recipients: &[Uuid]) {
        if recipients.is_empty() {
            return;
        }

        self.bridge
            .publish_to_users(recipients, WsPayload::ended_broadcast(broadcast_id, reason))
            .await;
    }
}

/// A publisher that announces nothing.
///
/// The default for a [`super::state::BroadcastState`] built without a bridge — the
/// service tests, and any deployment that has not wired realtime yet. Silent rather than
/// refusing: not being told a broadcast ended is a client that finds out on its next
/// read, and a module that could not run without Redis would be wrong for the flows that
/// do not use it (§4.6).
#[derive(Debug, Clone, Copy, Default)]
pub struct SilentEvents;

#[async_trait]
impl BroadcastEvents for SilentEvents {
    async fn ended(&self, _broadcast_id: Uuid, _reason: EndReason, _recipients: &[Uuid]) {}
}

#[cfg(any(test, feature = "test-support"))]
pub mod recording {
    //! The §10 double: an announcer that remembers what it was asked to send.

    use std::sync::{Mutex, MutexGuard};

    use async_trait::async_trait;
    use uuid::Uuid;

    use super::BroadcastEvents;
    use crate::modules::broadcast::model::EndReason;

    /// One announcement: which broadcast, the reason, and who was told.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct AnnouncedEnd {
        /// The broadcast that ended.
        pub broadcast_id: Uuid,
        /// Why.
        pub reason: EndReason,
        /// The accounts told directly.
        pub recipients: Vec<Uuid>,
    }

    /// Records every announcement, so a test can assert both that one happened and that
    /// one did **not**.
    #[derive(Debug, Default)]
    pub struct RecordingEvents {
        ended: Mutex<Vec<AnnouncedEnd>>,
    }

    impl RecordingEvents {
        /// An announcer that has said nothing.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Every announcement, oldest first.
        ///
        /// Not named `ended`: that is the trait method that *makes* one, and a reader
        /// who saw two of them would have to check which is which at every call site.
        #[must_use]
        pub fn announced(&self) -> Vec<AnnouncedEnd> {
            self.lock().clone()
        }

        /// Whether nothing has been announced.
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.lock().is_empty()
        }

        fn lock(&self) -> MutexGuard<'_, Vec<AnnouncedEnd>> {
            self.ended.lock().unwrap_or_else(|e| e.into_inner())
        }
    }

    #[async_trait]
    impl BroadcastEvents for RecordingEvents {
        async fn ended(&self, broadcast_id: Uuid, reason: EndReason, recipients: &[Uuid]) {
            self.lock().push(AnnouncedEnd {
                broadcast_id,
                reason,
                recipients: recipients.to_vec(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::recording::RecordingEvents;
    use super::*;

    #[tokio::test]
    async fn the_double_records_who_was_told_and_why() {
        // The reaper's fan-out is only assertable through this seam, so it is worth
        // proving the double keeps the recipients rather than just a count.
        let events = RecordingEvents::new();
        let broadcast = Uuid::from_u128(7);

        events
            .ended(
                broadcast,
                EndReason::HostDisconnected,
                &[Uuid::from_u128(1), Uuid::from_u128(2)],
            )
            .await;

        let announced = events.announced();
        assert_eq!(announced.len(), 1);
        assert_eq!(announced[0].broadcast_id, broadcast);
        assert_eq!(announced[0].reason, EndReason::HostDisconnected);
        assert_eq!(
            announced[0].recipients,
            [Uuid::from_u128(1), Uuid::from_u128(2)]
        );
    }

    #[tokio::test]
    async fn a_silent_publisher_accepts_an_announcement_and_says_nothing() {
        // The deployment without realtime must not fail a sweep.
        SilentEvents
            .ended(
                Uuid::from_u128(1),
                EndReason::Normal,
                &[Uuid::from_u128(2)],
            )
            .await;
    }
}
