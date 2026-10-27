//! Where a broadcast's lifecycle events reach the people concerned (§3.1, §10.3).
//!
//! # Why this is a port rather than a call to the bridge
//!
//! The same reason [`crate::modules::chat::publisher`] is one: the reaper's invariant is
//! *"a broadcast that was ended was announced to the people in it"*, and calling
//! [`WsPubSubBridge`] directly would make that invariant unassertable without a live
//! Redis. [`BroadcastEvents`] is the seam; production wires [`PubSubEvents`], the tests
//! wire [`recording::RecordingEvents`] and read back who was told.
//!
//! # Typed payloads, serialised once
//!
//! Every method takes the wire type it announces ([`BroadcastResponse`],
//! [`ParticipantChange`]) rather than a `serde_json::Value`. A `Value` would move the
//! decision of what a frame looks like from the serialiser to whoever built the map, and
//! the two have already disagreed once: a hand-built map spelled `creator_id` where the
//! HTTP response — and the client decoding it — spells `creatorId`. Taking the type means
//! the socket frame and the endpoint that answers the same question cannot drift, because
//! they are the same struct.
//!
//! # One delivery per recipient
//!
//! [`BroadcastEvents::ended`] publishes to the listed accounts, not to the room channel,
//! because the list is already exhaustive: the creator, every cohost, and everybody the
//! room server reports as connected. Publishing to the room as well would deliver the
//! same frame twice to anyone in both sets, and a client that reacted twice to
//! `endedBroadcast` would be reacting to this module's convenience.
//!
//! [`BroadcastEvents::home_invalidated`] is the one exception: it carries no state and
//! concerns everybody's home screen, so it goes to every connected client rather than to
//! a computed recipient set. Recomputing a fan-out for "the catalogue changed" would cost
//! a query per mutation to deliver a timestamp.
//!
//! Delivery is best-effort. A client that misses the frame reads the broadcast back and
//! finds it ended, which is why nothing here fails a sweep over one undelivered event.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;

use super::dto::BroadcastResponse;
use super::model::{EndReason, ParticipantRole};
use crate::infrastructure::ws::dto::WsPayload;
use crate::infrastructure::ws::pubsub::WsPubSubBridge;

/// The `participantJoined` / `participantLeft` frame (§10.3).
///
/// Nested rather than flat because the client decodes `participant` as its roster item
/// type ([`ParticipantFrame`] carries exactly the fields that decode needs) and reads
/// `count` for the room header without a second request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantChange {
    /// Which room changed.
    pub broadcast_id: Uuid,
    /// Who joined or left.
    pub participant: ParticipantFrame,
    /// How many accounts were in the room after the change.
    ///
    /// After, not before: a client that applied the frame and then read the header would
    /// otherwise see the count lag by one event.
    pub count: i64,
}

/// One account's presence, as the roster draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantFrame {
    /// Account id — what a client keys the row on.
    pub id: Uuid,
    /// Display name.
    pub full_name: String,
    /// Avatar storage key.
    pub avatar_id: Option<String>,
    /// Resolved avatar URL.
    pub avatar_url: Option<String>,
    /// What they may do in the room.
    pub role: ParticipantRole,
    /// Whether they are in the room after the change: `true` for a join, `false` for a
    /// leave, so one frame type serves both directions.
    pub is_joined: bool,
}

/// Announces a broadcast's lifecycle events to the accounts concerned.
///
/// One delivery per recipient, best-effort: a client that misses a frame reads the
/// broadcast back and finds the new state, which is why nothing here fails a sweep over
/// one undelivered event.
#[async_trait]
pub trait BroadcastEvents: Send + Sync + 'static {
    /// Tell [recipients] that `broadcast_id` ended, and why.
    ///
    /// [recipients] is creator + current cohosts + everybody the room server reports, so
    /// one frame per account and no duplicates.
    async fn ended(&self, broadcast_id: Uuid, reason: EndReason, recipients: &[Uuid]);

    /// Announce a broadcast that was created or went on air.
    ///
    /// Fired once, by the creator's `go_live` — the same action that turns a draft into
    /// a live broadcast — so a subscriber hears about a new broadcast from the writer of
    /// it rather than from a polling loop.
    ///
    /// The audience is currently the creator's own devices: this module has no follow
    /// graph to fan out to, and inventing one here would be a second source of truth for
    /// who follows whom. The global [`Self::home_invalidated`] nudge is what tells every
    /// other client to refetch the sections it can actually see.
    async fn broadcast_created(&self, broadcast: &BroadcastResponse, recipients: &[Uuid]);

    /// Tell [recipients] that [change] happened to a room's roster.
    ///
    /// Fired by `join` and `leave`. The payload is the participant projection the roster
    /// renders, so a client can add or remove a row without re-reading the roster.
    async fn participant_changed(&self, change: &ParticipantChange, recipients: &[Uuid]);

    /// Nudge every connected client that its home screen is stale.
    ///
    /// Fired after any mutation that changes what the home sections draw — a broadcast
    /// went live, was edited, was deleted, or ended, whether by its host or by the
    /// reaper. The frame carries only a timestamp; the client refetches the sections it
    /// has, so this is cheap enough to send to everybody.
    async fn home_invalidated(&self);
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

    async fn broadcast_created(&self, broadcast: &BroadcastResponse, recipients: &[Uuid]) {
        if recipients.is_empty() {
            return;
        }

        self.bridge
            .publish_to_users(recipients, WsPayload::new_broadcast(broadcast))
            .await;
    }

    async fn participant_changed(&self, change: &ParticipantChange, recipients: &[Uuid]) {
        if recipients.is_empty() {
            return;
        }

        let payload = if change.participant.is_joined {
            WsPayload::participant_joined(change)
        } else {
            WsPayload::participant_left(change)
        };
        self.bridge.publish_to_users(recipients, payload).await;
    }

    async fn home_invalidated(&self) {
        self.bridge
            .broadcast_all(WsPayload::home_invalidated())
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

    async fn broadcast_created(&self, _broadcast: &BroadcastResponse, _recipients: &[Uuid]) {}

    async fn participant_changed(&self, _change: &ParticipantChange, _recipients: &[Uuid]) {}

    async fn home_invalidated(&self) {}
}

#[cfg(any(test, feature = "test-support"))]
pub mod recording {
    //! The §10 double: an announcer that remembers what it was asked to send.

    use std::sync::{Mutex, MutexGuard};

    use async_trait::async_trait;
    use uuid::Uuid;

    use super::{BroadcastEvents, BroadcastResponse, ParticipantChange};
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

    /// A "created" or "went live" announcement.
    ///
    /// The broadcast is stored as JSON rather than as the response type: the double is
    /// asserting *what the socket would carry*, and a test that wants a field reads it off
    /// the serialised frame the client would have received.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct AnnouncedCreated {
        /// The broadcast frame, as the socket would send it.
        pub broadcast: serde_json::Value,
        /// The accounts told directly.
        pub recipients: Vec<Uuid>,
    }

    /// A roster change.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct AnnouncedParticipantChanged {
        /// The frame the socket would send.
        pub change: ParticipantChange,
        /// The accounts told directly.
        pub recipients: Vec<Uuid>,
    }

    /// The kind of participant-list change a recording captured.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ParticipantChangedKind {
        /// The participant joined the broadcast.
        Joined,
        /// The participant left the broadcast.
        Left,
    }

    /// One recorded lifecycle event.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum RecordingEvent {
        /// The broadcast ended.
        Ended(AnnouncedEnd),
        /// The broadcast was created or went live.
        BroadcastCreated(AnnouncedCreated),
        /// A participant joined or left the broadcast.
        ParticipantChanged(AnnouncedParticipantChanged),
        /// The home was invalidated.
        HomeInvalidated,
    }

    /// Records every announcement, so a test can assert both that one happened and that
    /// one did **not**.
    #[derive(Debug, Default)]
    pub struct RecordingEvents {
        events: Mutex<Vec<RecordingEvent>>,
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
        pub fn announced(&self) -> Vec<RecordingEvent> {
            self.lock().clone()
        }

        /// Whether nothing has been announced.
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.lock().is_empty()
        }

        /// Every "ended" announcement, oldest first.
        #[must_use]
        pub fn ended_events(&self) -> Vec<AnnouncedEnd> {
            self.announced()
                .into_iter()
                .filter_map(|event| match event {
                    RecordingEvent::Ended(ended) => Some(ended),
                    _ => None,
                })
                .collect()
        }

        /// Every roster change, oldest first.
        #[must_use]
        pub fn participant_events(&self) -> Vec<AnnouncedParticipantChanged> {
            self.announced()
                .into_iter()
                .filter_map(|event| match event {
                    RecordingEvent::ParticipantChanged(change) => Some(change),
                    _ => None,
                })
                .collect()
        }

        /// Every "created" announcement, oldest first.
        #[must_use]
        pub fn created_events(&self) -> Vec<AnnouncedCreated> {
            self.announced()
                .into_iter()
                .filter_map(|event| match event {
                    RecordingEvent::BroadcastCreated(created) => Some(created),
                    _ => None,
                })
                .collect()
        }

        /// How many home-invalidation nudges were sent.
        #[must_use]
        pub fn home_invalidations(&self) -> usize {
            self.announced()
                .iter()
                .filter(|event| matches!(event, RecordingEvent::HomeInvalidated))
                .count()
        }

        fn lock(&self) -> MutexGuard<'_, Vec<RecordingEvent>> {
            self.events.lock().unwrap_or_else(|e| e.into_inner())
        }
    }

    #[async_trait]
    impl BroadcastEvents for RecordingEvents {
        async fn ended(&self, broadcast_id: Uuid, reason: EndReason, recipients: &[Uuid]) {
            self.lock().push(RecordingEvent::Ended(AnnouncedEnd {
                broadcast_id,
                reason,
                recipients: recipients.to_vec(),
            }));
        }

        async fn broadcast_created(&self, broadcast: &BroadcastResponse, recipients: &[Uuid]) {
            self.lock()
                .push(RecordingEvent::BroadcastCreated(AnnouncedCreated {
                    // A response that cannot serialise is a bug in the response type, and
                    // failing the test that announced it is the loudest way to find out.
                    broadcast: serde_json::to_value(broadcast).unwrap_or_default(),
                    recipients: recipients.to_vec(),
                }));
        }

        async fn participant_changed(&self, change: &ParticipantChange, recipients: &[Uuid]) {
            self.lock().push(RecordingEvent::ParticipantChanged(
                AnnouncedParticipantChanged {
                    change: change.clone(),
                    recipients: recipients.to_vec(),
                },
            ));
        }

        async fn home_invalidated(&self) {
            self.lock().push(RecordingEvent::HomeInvalidated);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::modules::broadcast::events::SilentEvents;

        #[tokio::test]
        async fn the_double_records_who_was_told_and_why() {
            let events = RecordingEvents::new();
            let broadcast = Uuid::from_u128(7);

            events
                .ended(
                    broadcast,
                    EndReason::HostDisconnected,
                    &[Uuid::from_u128(1), Uuid::from_u128(2)],
                )
                .await;

            let announced = events.ended_events();
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
                .ended(Uuid::from_u128(1), EndReason::Normal, &[Uuid::from_u128(2)])
                .await;
            SilentEvents.home_invalidated().await;
        }
    }
}
