//! Where a stored chat event is announced to the room (§3.1).
//!
//! # Why this is a port rather than a call to the bridge
//!
//! The service's invariant is *"a message that is stored has been announced"*. Calling
//! [`WsPubSubBridge`] directly would make that invariant unassertable without a live
//! Redis, and it would put a concrete transport in the service's signature — the thing
//! `state.rs` exists to prevent (§5.6). [`ChatPublisher`] is the seam: production wires
//! [`PubSubPublisher`], the tests wire [`recording::RecordingPublisher`] and read back
//! exactly what was announced.
//!
//! # Delivery is best-effort, and that is on purpose
//!
//! [`ChatPublisher::publish`] returns nothing. The bridge already logs its own failures
//! and retries nothing, because a client that missed a frame is a client whose next page
//! read repairs it — whereas failing the request would turn a dropped hint into a lost
//! message. `master`'s service awaited the publish and ignored the result; this makes
//! the intent legible instead of implicit.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::infrastructure::ws::dto::WsPayload;
use crate::infrastructure::ws::pubsub::WsPubSubBridge;

/// Announces a chat event to everybody in a broadcast's room.
#[async_trait]
pub trait ChatPublisher: Send + Sync + 'static {
    /// Send `payload` to the room for `broadcast_id`, on whichever replica holds each
    /// socket.
    async fn publish(&self, broadcast_id: Uuid, payload: WsPayload);
}

/// The production publisher: the cross-replica pub/sub bridge.
///
/// Not the local socket registry, and the difference matters: a room's participants are
/// spread across replicas, so a message published to local sockets only would reach the
/// sender's neighbours and nobody else.
#[derive(Clone)]
pub struct PubSubPublisher {
    bridge: Arc<WsPubSubBridge>,
}

impl PubSubPublisher {
    /// Wrap the application's bridge.
    #[must_use]
    pub fn new(bridge: Arc<WsPubSubBridge>) -> Self {
        Self { bridge }
    }
}

impl std::fmt::Debug for PubSubPublisher {
    /// Names the field and stops: the bridge holds two Redis clients.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PubSubPublisher").finish_non_exhaustive()
    }
}

#[async_trait]
impl ChatPublisher for PubSubPublisher {
    async fn publish(&self, broadcast_id: Uuid, payload: WsPayload) {
        self.bridge.publish_to_room(broadcast_id, payload).await;
    }
}

#[cfg(any(test, feature = "test-support"))]
pub mod recording {
    //! The §10 double: a publisher that remembers what it was asked to send.

    use std::sync::{Mutex, MutexGuard};

    use async_trait::async_trait;
    use uuid::Uuid;

    use super::ChatPublisher;
    use crate::infrastructure::ws::dto::WsPayload;
    use crate::infrastructure::ws::model::WsEvent;

    /// Records every announcement, so a test can assert both that one happened and that
    /// one did **not**.
    ///
    /// The negative case is the one that needs a double: "a refused edit is not
    /// broadcast" is invisible from the client that sent it and would otherwise ship
    /// unnoticed.
    #[derive(Debug, Default)]
    pub struct RecordingPublisher {
        published: Mutex<Vec<(Uuid, WsPayload)>>,
    }

    impl RecordingPublisher {
        /// A publisher that has announced nothing.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Everything announced, oldest first.
        #[must_use]
        pub fn published(&self) -> Vec<(Uuid, WsPayload)> {
            self.lock().clone()
        }

        /// Just the event names, which is what most assertions are about.
        #[must_use]
        pub fn events(&self) -> Vec<WsEvent> {
            self.lock()
                .iter()
                .map(|(_, payload)| payload.event)
                .collect()
        }

        /// The most recent body for `event`, if one was announced.
        #[must_use]
        pub fn body_of(&self, event: WsEvent) -> Option<serde_json::Value> {
            self.lock()
                .iter()
                .rev()
                .find(|(_, payload)| payload.event == event)
                .map(|(_, payload)| payload.data.clone())
        }

        /// How many announcements were made.
        #[must_use]
        pub fn len(&self) -> usize {
            self.lock().len()
        }

        /// Whether nothing has been announced.
        #[must_use]
        pub fn is_empty(&self) -> bool {
            self.lock().is_empty()
        }

        fn lock(&self) -> MutexGuard<'_, Vec<(Uuid, WsPayload)>> {
            self.published.lock().unwrap_or_else(|e| e.into_inner())
        }
    }

    #[async_trait]
    impl ChatPublisher for RecordingPublisher {
        async fn publish(&self, broadcast_id: Uuid, payload: WsPayload) {
            self.lock().push((broadcast_id, payload));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::recording::RecordingPublisher;
    use super::*;
    use crate::infrastructure::ws::dto::WsPayload;
    use crate::infrastructure::ws::model::WsEvent;

    #[tokio::test]
    async fn the_double_records_what_was_announced_and_to_which_room() {
        // The double is the reason the service's fan-out invariant is testable at all,
        // so it is worth proving it records the room and not just the payload.
        let publisher = RecordingPublisher::new();
        let room = Uuid::from_u128(7);
        let other = Uuid::from_u128(8);

        publisher
            .publish(room, WsPayload::new_message(serde_json::json!({"id": "a"})))
            .await;
        publisher
            .publish(other, WsPayload::deleted_message(other, Uuid::from_u128(1)))
            .await;

        assert_eq!(publisher.len(), 2, "nothing is dropped");
        assert!(!publisher.is_empty());
        assert_eq!(
            publisher.events(),
            vec![WsEvent::NewMessage, WsEvent::DeletedMessage]
        );
        assert_eq!(publisher.published()[0].0, room, "the room is recorded");
        assert_eq!(
            publisher.body_of(WsEvent::NewMessage),
            Some(serde_json::json!({"id": "a"}))
        );
        assert!(publisher.body_of(WsEvent::DeletedMessage).is_some());
    }

    #[test]
    fn a_fresh_double_has_announced_nothing() {
        let publisher = RecordingPublisher::new();

        assert!(publisher.is_empty());
        assert!(publisher.events().is_empty());
        assert_eq!(publisher.body_of(WsEvent::NewMessage), None);
    }
}
