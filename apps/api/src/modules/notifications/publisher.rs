//! How an in-app notification reaches the recipient's sockets (§3.1).
//!
//! The seam is deliberately one method wide: this module decides *whether* a user
//! wants an in-app event (their `app_notifications` setting), and the adapter decides
//! how the event travels. On a single replica that would be the local registry; in
//! the deployment it is the Redis pub/sub bridge, because the recipient's sockets may
//! live on another replica — the same reasoning as [`crate::modules::chat::publisher`].
//!
//! # Why a trait rather than the bridge directly
//!
//! `master`'s service held the bridge and published inline, so a service test needed
//! a Redis. Every rule in this module — rendering, unread counting, push gating —
//! would then have been untestable without one. The trait is what lets
//! `tests/notifications_service.rs` assert *that* an event was published, and with
//! what text, using a recording double.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::infrastructure::ws::WsPayload;
use crate::infrastructure::ws::pubsub::WsPubSubBridge;

/// Delivers an in-app notification to one user.
#[async_trait]
pub trait Notifier: Send + Sync + std::fmt::Debug {
    /// Publish `title`/`body` to every socket `owner_id` holds.
    ///
    /// Never returns an error: a notification that was stored is already delivered as
    /// far as the inbox is concerned, and a socket that cannot be reached is not a
    /// reason to undo the row. Failures are logged by the adapter.
    async fn notify(&self, owner_id: Uuid, title: &str, body: &str);
}

/// The real adapter: the cross-replica WebSocket bridge.
#[derive(Clone)]
pub struct PubSubNotifier {
    bridge: Arc<WsPubSubBridge>,
}

impl std::fmt::Debug for PubSubNotifier {
    /// Names the field and stops: the bridge holds Redis clients, whose config
    /// carries the URL.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PubSubNotifier")
            .field("bridge", &"[pub/sub]")
            .finish()
    }
}

impl PubSubNotifier {
    /// Publish through `bridge`.
    #[must_use]
    pub fn new(bridge: Arc<WsPubSubBridge>) -> Self {
        Self { bridge }
    }
}

#[async_trait]
impl Notifier for PubSubNotifier {
    async fn notify(&self, owner_id: Uuid, title: &str, body: &str) {
        self.bridge
            .publish_to_user(owner_id, WsPayload::notification(owner_id, title, body))
            .await;
    }
}

/// A [`Notifier`] that remembers what it was asked to publish.
///
/// §5.6's in-memory double, and the reason the service suite can assert the *text* a
/// user would see on their socket — which is the part of a notification that
/// regresses, not the transport.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Default)]
pub struct RecordingNotifier {
    sent: std::sync::Mutex<Vec<(Uuid, String, String)>>,
}

#[cfg(any(test, feature = "test-support"))]
impl RecordingNotifier {
    /// An empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every event published so far, in order: `(recipient, title, body)`.
    #[must_use]
    pub fn sent(&self) -> Vec<(Uuid, String, String)> {
        self.sent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// How many events were published.
    #[must_use]
    pub fn count(&self) -> usize {
        self.sent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

#[cfg(any(test, feature = "test-support"))]
#[async_trait]
impl Notifier for RecordingNotifier {
    async fn notify(&self, owner_id: Uuid, title: &str, body: &str) {
        self.sent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((owner_id, title.to_owned(), body.to_owned()));
    }
}
