//! Who is subscribed to a broadcast's room, as the service sees it (§3.1, §10.3).
//!
//! # Why this is a port rather than a call to [`WsService`]
//!
//! The service decides *when* membership changes — a roster row was written, a leave
//! closed it, a broadcast ended — and the socket layer decides *how*. Calling the socket
//! registry from here would make every service test need Redis, and it would put a
//! `WsPubSubBridge` in a module whose one rule is that only `state.rs` names a concrete
//! adapter.
//!
//! [`super::events::BroadcastEvents`] is the same shape for the same reason: the seam is
//! what lets `tests/broadcast_service.rs` assert that a join subscribed the room without
//! a live socket anywhere.
//!
//! # Best effort, never fatal
//!
//! The database is the source of truth. A replica that failed to subscribe still
//! delivers to its own sockets, and a client that missed a frame reads the room back.
//! So a failure is logged by the adapter rather than returned: a join whose row is
//! already written must not be reported as failed because one `SADD` timed out.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::infrastructure::ws::WsService;
use crate::infrastructure::ws::pubsub::WsPubSubBridge;

/// The membership changes a broadcast flow asks for.
///
/// One method per lifecycle edge rather than a single `set_member(bool, …)`: the three
/// cases are not symmetric — an ending drops *everybody*, including accounts this
/// replica was never told about — and a boolean would make the caller's intent read as
/// "toggle" for what is really "the broadcast is over".
#[async_trait]
pub trait BroadcastRooms: Send + Sync + 'static {
    /// `user_id` is now in `broadcast_id`'s room.
    async fn joined(&self, user_id: Uuid, broadcast_id: Uuid);

    /// `user_id` is no longer in `broadcast_id`'s room.
    async fn left(&self, user_id: Uuid, broadcast_id: Uuid);

    /// The broadcast is over: drop every local member and the room subscription.
    ///
    /// The per-user [`Self::left`] would be enough to make the bookkeeping true, but
    /// only this releases the replica's channel subscription when the last member goes —
    /// otherwise a replica that once served a room keeps listening to it all day.
    async fn closed(&self, broadcast_id: Uuid);
}

/// The production adapter: the local registry plus the cross-replica bridge.
///
/// Goes through [`WsService`] rather than the bridge directly so there is one place that
/// knows room membership has a local half and a shared half.
#[derive(Clone)]
pub struct WsRooms {
    /// This replica's sockets and rooms.
    ws: WsService,
    /// The shared membership SET and the room channel subscription.
    bridge: Arc<WsPubSubBridge>,
}

impl WsRooms {
    /// Wrap the application's socket layer.
    #[must_use]
    pub fn new(ws: WsService, bridge: Arc<WsPubSubBridge>) -> Self {
        Self { ws, bridge }
    }
}

impl std::fmt::Debug for WsRooms {
    /// Names the fields and stops: the bridge holds Redis clients whose config carries
    /// the URL.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WsRooms")
            .field("ws", &"[sockets]")
            .field("bridge", &"[pub/sub]")
            .finish()
    }
}

#[async_trait]
impl BroadcastRooms for WsRooms {
    async fn joined(&self, user_id: Uuid, broadcast_id: Uuid) {
        self.ws.join_room(user_id, broadcast_id, &self.bridge).await;
    }

    async fn left(&self, user_id: Uuid, broadcast_id: Uuid) {
        self.ws
            .leave_room(user_id, broadcast_id, &self.bridge)
            .await;
    }

    async fn closed(&self, broadcast_id: Uuid) {
        self.ws.close_room(broadcast_id, &self.bridge).await;
    }
}

/// A membership port that does nothing.
///
/// The default for a [`super::state::BroadcastState`] built without a bridge — the
/// service tests, and any deployment that has not wired realtime yet. Silent rather
/// than refusing: joining a room whose sockets nobody holds is not a failure, and a
/// module that could not run without Redis would be wrong for the flows that do not use
/// it (§4.6).
#[derive(Debug, Clone, Copy, Default)]
pub struct SilentRooms;

#[async_trait]
impl BroadcastRooms for SilentRooms {
    async fn joined(&self, _user_id: Uuid, _broadcast_id: Uuid) {}

    async fn left(&self, _user_id: Uuid, _broadcast_id: Uuid) {}

    async fn closed(&self, _broadcast_id: Uuid) {}
}

#[cfg(any(test, feature = "test-support"))]
pub mod recording {
    //! The §10 double: a membership port that remembers what it was asked to do.
    //!
    //! Kept in the same order the service asked, because "joined before the token was
    //! minted" and "joined after" are different bugs and only the order can tell them
    //! apart.

    use std::sync::{Mutex, MutexGuard};

    use async_trait::async_trait;
    use uuid::Uuid;

    use super::BroadcastRooms;

    /// Every membership change, as `(user_id, broadcast_id)` pairs.
    #[derive(Debug, Default)]
    pub struct RecordingRooms {
        /// Joins, in call order.
        joined: Mutex<Vec<(Uuid, Uuid)>>,
        /// Leaves, in call order.
        left: Mutex<Vec<(Uuid, Uuid)>>,
        /// Closed broadcasts, in call order.
        closed: Mutex<Vec<Uuid>>,
    }

    impl RecordingRooms {
        /// An empty recorder.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// `(user_id, broadcast_id)` for every join the service asked for.
        #[must_use]
        pub fn joined(&self) -> Vec<(Uuid, Uuid)> {
            lock(&self.joined).clone()
        }

        /// `(user_id, broadcast_id)` for every leave the service asked for.
        #[must_use]
        pub fn left(&self) -> Vec<(Uuid, Uuid)> {
            lock(&self.left).clone()
        }

        /// Every broadcast the service closed.
        #[must_use]
        pub fn closed(&self) -> Vec<Uuid> {
            lock(&self.closed).clone()
        }
    }

    /// A poisoned lock is recovered rather than panicked on: this is a test double, and
    /// a failing assertion elsewhere must not turn into a second failure here.
    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[async_trait]
    impl BroadcastRooms for RecordingRooms {
        async fn joined(&self, user_id: Uuid, broadcast_id: Uuid) {
            lock(&self.joined).push((user_id, broadcast_id));
        }

        async fn left(&self, user_id: Uuid, broadcast_id: Uuid) {
            lock(&self.left).push((user_id, broadcast_id));
        }

        async fn closed(&self, broadcast_id: Uuid) {
            lock(&self.closed).push(broadcast_id);
        }
    }
}
