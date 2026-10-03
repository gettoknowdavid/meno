//! WebSocket delivery: the local socket registry and the frames sent over it.
//!
//! Ported from `apps/api/src/shared/services/ws/mod.rs` on `master` (`903c3ba`),
//! moved to the `infrastructure::` layout with the imports rewritten and the §3.2
//! mandatory-TTL rule applied.
//!
//! # Layering
//!
//! This is the *local* half of delivery and owns no Redis connection of its own beyond
//! the shared one it buffers offline messages into. Cross-instance fan-out belongs to
//! [`pubsub::WsPubSubBridge`]; application code that needs to reach a user on another
//! replica must go through the bridge, never through here.
//!
//! # `handlers` is not ported yet
//!
//! `master`'s `handlers.rs` drives the axum upgrade, the heartbeat and the read loop.
//! It reaches into `crate::state::MenoState`, `crate::modules::{auth,broadcast,chat}`
//! and `crate::jobs::broadcast_jobs`, none of which exist in the monorepo yet — they
//! arrive with Step 3.7's `bootstrap`/`state`/`modules`/`routes`. It is therefore not
//! declared here. When it lands it needs, from this module:
//!
//! - [`WsService::register`] / [`WsService::unregister`] — the connection registry.
//! - [`WsService::join_room`] / [`WsService::leave_room`] — room membership.
//! - [`WsService::drain_message_buffer`] — offline replay on reconnect.
//! - [`dto::ClientMessage`] and [`model::WsEvent::is_client_to_server`] — frame
//!   dispatch, so a client cannot drive a server-only event.
//!
//! Plus the new `TTL_`-free Redis writes: presence, reconnect rate and grace-period
//! keys all go through [`crate::infrastructure::redis::RedisKey`], which now carries
//! its own expiry, so the `set_ex`-and-a-second-expiry-argument pattern from `master`
//! no longer compiles.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use dashmap::{DashMap, DashSet};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::infrastructure::constants::{
    MAX_WS_CONNECTIONS_PER_USER, MESSAGE_BUFFER_SIZE, MESSAGE_BUFFER_TTL_SECS,
    OFFLINE_MESSAGE_HISTORY,
};
use crate::infrastructure::redis::Redis;
use crate::infrastructure::redis::keys::{RedisKey, ttl};

pub mod dto;
pub mod errors;
pub mod model;
pub mod pubsub;

pub use dto::{ClientMessage, WsPayload, WsQuery};
pub use errors::{WsError, WsErrorCode};
pub use model::{GracePeriodConfig, HeartbeatConfig, WsEvent};

/// One open socket for one user.
///
/// A user normally has several (web plus mobile), so delivery goes to all of them;
/// `conn_id` is what lets a single one be removed on disconnect.
#[derive(Clone, Debug)]
pub struct ConnectionSender {
    /// Identifier issued at registration, unique within the process.
    pub conn_id: usize,
    /// Outbound channel for this socket.
    sender: mpsc::Sender<Arc<WsPayload>>,
}

impl ConnectionSender {
    /// This connection's identifier.
    #[must_use]
    pub const fn conn_id(&self) -> usize {
        self.conn_id
    }
}

/// The in-process socket registry: who is connected here, and which rooms they are in.
///
/// Deliberately free of any I/O. This is the whole of the "which sockets live on this
/// replica" question, and keeping it separate from [`WsService`] means it can be
/// tested exhaustively without a Redis server — the registry is where the connection
/// cap and the subscribe/unsubscribe decisions live, so it is also where a bug is
/// easiest to introduce and hardest to notice.
///
/// ## Responsibilities
///
/// - **Connections** — which users have sockets here ([`Self::clients`]).
/// - **Rooms** — which users are in which room here, mirroring the shared membership
///   SET for the delivery fast path ([`Self::rooms`]).
/// - **Subscription bookkeeping** — [`Self::add_local_room_member`] and
///   [`Self::remove_local_room_member`] answer "does this replica need the room
///   channel?", which is what the bridge uses to decide when to subscribe.
///
/// ## What it deliberately does not do
///
/// Cross-instance delivery. Every method here touches only local state, so calling one
/// never reaches another replica. Use [`pubsub::WsPubSubBridge`] for that.
#[derive(Clone, Default)]
pub struct Registry {
    /// `user_id` → this replica's open sockets for them.
    clients: Arc<DashMap<Uuid, Vec<ConnectionSender>>>,

    /// `broadcast_id` → users in that room on this replica.
    rooms: Arc<DashMap<Uuid, DashSet<Uuid>>>,

    /// Source of `conn_id`s. Monotonic and never reused.
    conn_seq: Arc<AtomicUsize>,
}

impl Registry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a socket for `user_id`.
    ///
    /// Returns the new `conn_id`, or `None` if the user is already at
    /// [`MAX_WS_CONNECTIONS_PER_USER`] — the per-user cap stops one client from
    /// pinning thousands of channels on a memory-capped replica (§3.2).
    pub fn register(&self, user_id: Uuid, sender: mpsc::Sender<Arc<WsPayload>>) -> Option<usize> {
        let mut entry = self.clients.entry(user_id).or_default();

        if entry.len() >= MAX_WS_CONNECTIONS_PER_USER {
            tracing::warn!(
                user_id = %user_id,
                connections = entry.len(),
                "WS connection limit reached — rejecting new connection"
            );
            return None;
        }

        let conn_id = self.conn_seq.fetch_add(1, Ordering::Relaxed);
        entry.push(ConnectionSender { conn_id, sender });

        tracing::debug!(user_id = %user_id, conn_id, "WS connection registered");
        Some(conn_id)
    }

    /// Remove one socket, and the user entirely if it was their last.
    ///
    /// The entry is removed rather than left empty so [`Self::is_online`] stops
    /// reporting `true` the moment a user disconnects, which is what presence and
    /// "now live" fan-outs branch on.
    pub fn unregister(&self, user_id: Uuid, conn_id: usize) {
        if let Some(mut entry) = self.clients.get_mut(&user_id) {
            entry.retain(|c| c.conn_id != conn_id);
            if entry.is_empty() {
                drop(entry);
                self.clients.remove(&user_id);
            }
        }
        tracing::debug!(user_id = %user_id, conn_id, "WS connection unregistered");
    }

    /// Record a local room member, reporting whether this replica had none before.
    ///
    /// `true` means the replica now needs a subscription to the room channel.
    #[must_use]
    pub fn add_local_room_member(&self, broadcast_id: Uuid, user_id: Uuid) -> bool {
        let room = self.rooms.entry(broadcast_id).or_default();
        let was_empty = room.is_empty();
        room.insert(user_id);
        was_empty
    }

    /// Remove a local room member, reporting whether that emptied the room.
    ///
    /// `true` means the replica can drop its subscription.
    #[must_use]
    pub fn remove_local_room_member(&self, broadcast_id: Uuid, user_id: Uuid) -> bool {
        let mut became_empty = false;
        if let Some(room) = self.rooms.get(&broadcast_id) {
            room.remove(&user_id);
            became_empty = room.is_empty();
        }
        if became_empty {
            self.rooms.remove(&broadcast_id);
        }
        became_empty
    }

    /// Drop every local member of a room, returning who they were.
    ///
    /// Used when this replica learns the broadcast has ended. Returning the users lets
    /// the caller notify or clean up per-user state.
    #[must_use]
    pub fn clear_local_room(&self, broadcast_id: Uuid) -> Vec<Uuid> {
        self.rooms
            .remove(&broadcast_id)
            .map(|(_, members)| members.iter().map(|u| *u).collect())
            .unwrap_or_default()
    }

    /// Every local member of a room.
    #[must_use]
    pub fn room_members(&self, broadcast_id: Uuid) -> Vec<Uuid> {
        self.rooms
            .get(&broadcast_id)
            .map(|members| members.iter().map(|u| *u).collect())
            .unwrap_or_default()
    }

    /// Whether the user has at least one socket on this replica.
    #[must_use]
    pub fn is_online(&self, user_id: Uuid) -> bool {
        self.clients.contains_key(&user_id)
    }

    /// How many sockets the user has here.
    #[must_use]
    pub fn connection_count(&self, user_id: Uuid) -> usize {
        self.clients
            .get(&user_id)
            .map_or(0, |senders| senders.len())
    }

    /// Every user with a socket on this replica.
    ///
    /// For "now live" fan-outs that should reach only currently-connected users,
    /// rather than everyone who ever connected.
    #[must_use]
    pub fn online_users(&self) -> Vec<Uuid> {
        self.clients.iter().map(|entry| *entry.key()).collect()
    }

    /// Deliver to every local socket in a room.
    ///
    /// Called by the bridge after a message arrives from Redis, so the Redis hop has
    /// already done the cross-instance routing. Purely local: no Redis call here, so a
    /// room fan-out costs nothing beyond the `DashMap` walk.
    ///
    /// Returns the number of sockets written to.
    pub async fn send_to_room(&self, broadcast_id: Uuid, payload: WsPayload) -> usize {
        let Some(room) = self.rooms.get(&broadcast_id) else {
            return 0;
        };

        let arc = Arc::new(payload);
        let mut delivered = 0usize;
        for user_id in room.iter() {
            if let Some(senders) = self.clients.get(&*user_id) {
                for sender in senders.iter() {
                    // Dropped rather than propagated: see [`Self::send_to_sockets`].
                    if sender.sender.send(Arc::clone(&arc)).await.is_ok() {
                        delivered += 1;
                    } else {
                        tracing::debug!(
                            user_id = %*user_id,
                            broadcast_id = %broadcast_id,
                            "send failed; socket is gone"
                        );
                    }
                }
            }
        }
        delivered
    }

    /// Deliver to every local socket a user has.
    ///
    /// Returns `false` when the user has no socket here, which is the caller's cue to
    /// buffer instead. Returning a bool rather than deciding here is what keeps the
    /// I/O-free registry testable: buffering is [`WsService`]'s job.
    pub async fn send_to_sockets(&self, user_id: Uuid, payload: WsPayload) -> bool {
        let Some(senders) = self.clients.get(&user_id) else {
            return false;
        };

        let arc = Arc::new(payload);
        for sender in senders.iter() {
            if sender.sender.send(Arc::clone(&arc)).await.is_err() {
                tracing::debug!(user_id = %user_id, "send failed; socket is gone");
            }
        }
        true
    }

    /// Deliver to several users, reporting which had no local socket.
    ///
    /// The returned list is the set that needs buffering.
    pub async fn send_to_sockets_many(&self, user_ids: &[Uuid], payload: WsPayload) -> Vec<Uuid> {
        let arc = Arc::new(payload);
        let mut absent = Vec::new();

        for &uid in user_ids {
            let Some(senders) = self.clients.get(&uid) else {
                absent.push(uid);
                continue;
            };
            for sender in senders.iter() {
                if sender.sender.send(Arc::clone(&arc)).await.is_err() {
                    tracing::debug!(user_id = %uid, "send failed; socket is gone");
                }
            }
        }

        absent
    }

    /// Deliver to every locally connected user.
    ///
    /// Returns the number of sockets written to.
    pub async fn broadcast_all(&self, payload: WsPayload) -> usize {
        let arc = Arc::new(payload);
        let mut delivered = 0usize;

        for entry in self.clients.iter() {
            for sender in entry.value() {
                if sender.sender.send(Arc::clone(&arc)).await.is_ok() {
                    delivered += 1;
                }
            }
        }
        delivered
    }
}

/// The local delivery layer: a [`Registry`] plus the Redis buffer for absent users.
#[derive(Clone)]
pub struct WsService {
    /// Local sockets and room membership.
    registry: Registry,

    /// Shared client, used only to buffer messages for absent users.
    redis: Redis,
}

impl WsService {
    /// Build a delivery layer over `redis`.
    #[must_use]
    pub fn new(redis: Redis) -> Self {
        Self {
            registry: Registry::new(),
            redis,
        }
    }

    /// The socket registry, for callers that need presence or room membership without
    /// any delivery.
    #[must_use]
    pub const fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Add a user to a room here **and** in the shared membership SET.
    ///
    /// Call from the HTTP join endpoint, and from the socket handler when a user
    /// reconnects to a broadcast that is still live. Idempotent on both sides, so a
    /// reconnect that re-joins is harmless.
    pub async fn join_room(
        &self,
        user_id: Uuid,
        broadcast_id: Uuid,
        bridge: &pubsub::WsPubSubBridge,
    ) {
        // The bool means "subscribe?" — the bridge asks that itself, once it has
        // reached the shared SET. Here we only need the local membership recorded.
        let _ = self.registry.add_local_room_member(broadcast_id, user_id);

        if let Err(e) = bridge.join_room(user_id, broadcast_id).await {
            // Local delivery still works without the shared SET, so this is degraded
            // rather than broken: cross-instance membership checks and counts will
            // miss this user until it succeeds.
            tracing::warn!(
                error = %e,
                user_id = %user_id,
                broadcast_id = %broadcast_id,
                "failed to join room in Redis — local delivery still works"
            );
        }
    }

    /// Remove a user from a room here **and** in the shared membership SET.
    ///
    /// Call from the HTTP leave endpoint, from `end` when a broadcast finishes, and
    /// from the socket handler's cleanup.
    pub async fn leave_room(
        &self,
        user_id: Uuid,
        broadcast_id: Uuid,
        bridge: &pubsub::WsPubSubBridge,
    ) {
        let _ = self
            .registry
            .remove_local_room_member(broadcast_id, user_id);

        if let Err(e) = bridge.leave_room(user_id, broadcast_id).await {
            tracing::warn!(
                error = %e,
                user_id = %user_id,
                broadcast_id = %broadcast_id,
                "failed to leave room in Redis"
            );
        }
    }

    /// Deliver to every local socket in a room.
    ///
    /// Delegates to the registry; returns how many sockets were written to.
    pub async fn send_to_room(&self, broadcast_id: Uuid, payload: WsPayload) -> usize {
        self.registry.send_to_room(broadcast_id, payload).await
    }

    /// Deliver to a user's local sockets, or buffer it if they have none here.
    ///
    /// A user with no socket on *this* replica may well be connected to another one,
    /// so this is not "offline" — the message goes to the shared buffer and is either
    /// replayed by the owning replica or expires (§9.4).
    pub async fn send_to_user(&self, user_id: Uuid, payload: WsPayload) {
        if self
            .registry
            .send_to_sockets(user_id, payload.clone())
            .await
        {
            return;
        }

        self.buffer_message(user_id, payload).await;
    }

    /// Deliver to several users, buffering for those with no local socket.
    ///
    /// Local-only. For cross-instance fan-out use
    /// [`pubsub::WsPubSubBridge::publish_to_users`].
    pub async fn send_to_users(&self, user_ids: &[Uuid], payload: WsPayload) {
        let absent = self
            .registry
            .send_to_sockets_many(user_ids, payload.clone())
            .await;

        for user_id in absent {
            self.buffer_message(user_id, payload.clone()).await;
        }
    }

    /// Deliver to every locally connected user.
    ///
    /// Returns the number of sockets written to.
    pub async fn broadcast_all(&self, payload: WsPayload) -> usize {
        self.registry.broadcast_all(payload).await
    }

    /// Send a room-scoped error to one user.
    pub async fn send_error(
        &self,
        user_id: Uuid,
        broadcast_id: Uuid,
        code: WsErrorCode,
        message: impl Into<String>,
    ) {
        self.send_to_user(user_id, WsPayload::error(broadcast_id, code, message))
            .await;
    }

    /// Send a connection-scoped error to one user.
    ///
    /// Used for failures that belong to no room, so `broadcastId` is absent rather
    /// than `Uuid::nil()`.
    pub async fn send_connection_error(&self, user_id: Uuid, message: impl Into<String>) {
        self.send_to_user(user_id, WsPayload::unsupported_error(message))
            .await;
    }

    /// Buffer a message for a user with no local socket.
    ///
    /// `LPUSH` + `LTRIM` keeps only the newest [`OFFLINE_MESSAGE_HISTORY`] messages,
    /// with the key's own TTL as a backstop (§9.4 — this buffer shares the
    /// memory-capped, persistence-free instance with everything else).
    pub async fn buffer_message(&self, user_id: Uuid, payload: WsPayload) {
        let key = RedisKey::ws_buffer(user_id, ttl::WS_BUFFER);

        let json = match serde_json::to_string(&payload) {
            Ok(json) => json,
            Err(e) => {
                tracing::error!(error = %e, user_id = %user_id, "failed to serialise buffered message");
                return;
            }
        };

        // `lpush` applies the TTL when the key is created; re-applying it on every push
        // would keep the key alive forever, which is the failure §9.4 warns about.
        if let Err(e) = self.redis.lpush(&key, &json).await {
            tracing::warn!(error = %e, user_id = %user_id, "failed to buffer message");
            return;
        }

        // -1 as the stop index means "through the oldest retained", because `LPUSH`
        // puts new messages at the head.
        if let Err(e) = self
            .redis
            .ltrim(&key, 0, (OFFLINE_MESSAGE_HISTORY - 1) as i64)
            .await
        {
            tracing::warn!(error = %e, user_id = %user_id, "failed to trim message buffer");
        }
    }

    /// Drain a user's offline buffer, oldest first.
    ///
    /// One Lua script so the read and the delete are atomic: two sockets reconnecting
    /// concurrently would otherwise both read the same messages and deliver them
    /// twice.
    ///
    /// Returns an empty vector on any failure. A lost replay is recoverable — the
    /// client refetches on reconnect — whereas a failed request is not.
    pub async fn drain_message_buffer(&self, user_id: Uuid) -> Vec<WsPayload> {
        let key = RedisKey::ws_buffer(user_id, ttl::WS_BUFFER);

        const DRAIN: &str = r"
            local items = redis.call('LRANGE', KEYS[1], 0, -1)
            redis.call('DEL', KEYS[1])
            return items
        ";

        let items: Vec<String> = match self
            .redis
            .eval::<Vec<String>, _, _>(DRAIN, vec![key.as_ref()], Vec::<()>::new())
            .await
        {
            Ok(items) => items,
            Err(e) => {
                tracing::error!(error = %e, user_id = %user_id, "failed to drain message buffer");
                return Vec::new();
            }
        };

        // `LPUSH` puts newest first, so reversing restores chronological order — which
        // is the order a client expects to replay them in.
        items
            .into_iter()
            .rev()
            .filter_map(|json| match serde_json::from_str::<WsPayload>(&json) {
                Ok(payload) => Some(payload),
                Err(e) => {
                    tracing::warn!(error = %e, user_id = %user_id, "dropping unparseable buffered message");
                    None
                }
            })
            .collect()
    }

    /// Tell every connected client this replica is going away.
    ///
    /// Clients reconnect to another replica, so the event is `recoverable`; the delay
    /// gives them a moment to start before the socket is dropped.
    pub async fn close_all_connections(&self) {
        self.broadcast_all(WsPayload::server_shutdown()).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    /// Re-exported so callers read `ws::MESSAGE_BUFFER_TTL_SECS` rather than reaching
    /// into `infrastructure::constants`.
    ///
    /// Exists because the buffer's lifetime is part of this module's contract, and a
    /// caller should not have to know where the constant lives.
    #[must_use]
    pub const fn message_buffer_ttl_secs() -> i64 {
        MESSAGE_BUFFER_TTL_SECS
    }

    /// The per-connection outbound channel depth.
    #[must_use]
    pub const fn message_buffer_size() -> usize {
        MESSAGE_BUFFER_SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    /// A channel plus its receiver, so a test can both register and then read.
    fn channel() -> (mpsc::Sender<Arc<WsPayload>>, mpsc::Receiver<Arc<WsPayload>>) {
        mpsc::channel(MESSAGE_BUFFER_SIZE)
    }

    /// A registry with no Redis behind it.
    ///
    /// [`Registry`] is deliberately I/O-free, so every test here runs with no
    /// infrastructure at all. The tests that need Redis live in
    /// `redis::coalescing::tests::live`.
    fn registry() -> Registry {
        Registry::new()
    }

    #[tokio::test]
    async fn registering_then_unregistering_marks_the_user_offline() {
        let registry = registry();
        let user = id(1);
        let (tx, mut rx) = channel();

        assert!(!registry.is_online(user));

        let conn_id = registry.register(user, tx).expect("first connection");
        assert!(registry.is_online(user));
        assert_eq!(registry.connection_count(user), 1);

        // And the socket really receives what is sent to it.
        assert!(
            registry
                .send_to_sockets(user, WsPayload::host_reconnected(id(9)))
                .await
        );
        assert_eq!(
            rx.recv().await.expect("payload").event,
            WsEvent::HostReconnected
        );

        registry.unregister(user, conn_id);
        assert!(
            !registry.is_online(user),
            "the last disconnect must clear the entry"
        );
        assert_eq!(registry.connection_count(user), 0);
    }

    #[tokio::test]
    async fn unregistering_one_socket_leaves_the_users_others_alive() {
        // Web and mobile hold separate sockets; closing one must not log the user out
        // of the other.
        let registry = registry();
        let user = id(1);
        let (tx_web, _rx_web) = channel();
        let (tx_mobile, mut rx_mobile) = channel();

        let web = registry.register(user, tx_web).expect("web");
        let mobile = registry.register(user, tx_mobile).expect("mobile");
        assert_eq!(registry.connection_count(user), 2);
        assert_ne!(web, mobile, "conn_ids must be unique");

        registry.unregister(user, web);
        assert!(registry.is_online(user));
        assert_eq!(registry.connection_count(user), 1);

        registry
            .send_to_sockets(user, WsPayload::host_reconnected(id(9)))
            .await;
        assert!(
            rx_mobile.recv().await.is_some(),
            "the surviving socket must still receive"
        );
    }

    #[tokio::test]
    async fn the_per_user_connection_cap_is_enforced() {
        let registry = registry();
        let user = id(1);

        for i in 0..MAX_WS_CONNECTIONS_PER_USER {
            let (tx, _rx) = channel();
            assert!(
                registry.register(user, tx).is_some(),
                "connection {i} should be within the cap"
            );
        }

        let (tx, _rx) = channel();
        assert!(
            registry.register(user, tx).is_none(),
            "the {MAX_WS_CONNECTIONS_PER_USER}th connection must be refused"
        );
        assert_eq!(registry.connection_count(user), MAX_WS_CONNECTIONS_PER_USER);
    }

    #[tokio::test]
    async fn the_connection_cap_is_per_user_not_global() {
        // One user hitting the cap must not cost anyone else their socket.
        let registry = registry();
        let noisy = id(1);
        let quiet = id(2);

        for _ in 0..MAX_WS_CONNECTIONS_PER_USER {
            let (tx, _rx) = channel();
            registry.register(noisy, tx).expect("within cap");
        }
        let (tx, _rx) = channel();
        assert!(registry.register(noisy, tx).is_none());

        let (tx, _rx) = channel();
        assert!(
            registry.register(quiet, tx).is_some(),
            "a different user has their own budget"
        );
    }

    #[tokio::test]
    async fn connection_ids_are_unique_and_never_reused() {
        // A reused id would let one socket's unregister remove another's.
        let registry = registry();
        let mut ids = Vec::new();

        for _ in 0..10 {
            let (tx, _rx) = channel();
            let user = id(1);
            let conn_id = registry.register(user, tx).expect("register");
            ids.push(conn_id);
            registry.unregister(user, conn_id);
        }

        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "conn_ids must never repeat");
    }

    #[tokio::test]
    async fn unregistering_an_unknown_connection_is_harmless() {
        let registry = registry();
        let user = id(1);
        let (tx, _rx) = channel();
        let conn_id = registry.register(user, tx).expect("register");

        registry.unregister(user, conn_id + 999);
        assert!(
            registry.is_online(user),
            "removing a stale id must not evict a live socket"
        );
    }

    #[tokio::test]
    async fn connection_senders_expose_their_id() {
        // The id is what `unregister` keys on, so it must be readable back.
        let registry = registry();
        let (tx, _rx) = channel();
        let conn_id = registry.register(id(1), tx).expect("register");

        let senders = registry.clients.get(&id(1)).expect("registered");
        assert_eq!(senders[0].conn_id(), conn_id);
    }

    #[tokio::test]
    async fn online_users_lists_everyone_with_a_socket() {
        let registry = registry();
        let a = id(1);
        let b = id(2);

        for user in [a, b] {
            let (tx, _rx) = channel();
            registry.register(user, tx).expect("register");
        }

        // `DashMap` iterates in arbitrary order, so every assertion sorts first.
        let online = |registry: &Registry| {
            let mut users = registry.online_users();
            users.sort_unstable();
            users
        };

        assert_eq!(online(&registry), vec![a, b]);

        // A reconnecting user holds two sockets until the old one is reaped; only
        // when both are gone do they drop off the online list.
        let (tx, _rx) = channel();
        let reconnected = registry.register(a, tx).expect("re-register");
        assert_eq!(registry.connection_count(a), 2);

        registry.unregister(a, reconnected);
        assert_eq!(online(&registry), vec![a, b], "one socket remains");

        let stale = registry.clients.get(&a).expect("still registered")[0].conn_id();
        registry.unregister(a, stale);
        assert_eq!(online(&registry), vec![b]);
    }

    #[tokio::test]
    async fn the_first_local_room_member_triggers_a_subscribe() {
        let registry = registry();
        let room = id(100);

        assert!(
            registry.add_local_room_member(room, id(1)),
            "the first member means this replica needs the channel"
        );
        assert!(
            !registry.add_local_room_member(room, id(2)),
            "a second member needs no new subscription"
        );
    }

    #[tokio::test]
    async fn only_the_last_local_room_member_triggers_an_unsubscribe() {
        let registry = registry();
        let room = id(100);

        assert!(registry.add_local_room_member(room, id(1)));
        assert!(!registry.add_local_room_member(room, id(2)));

        assert!(
            !registry.remove_local_room_member(room, id(1)),
            "one member remains, keep the subscription"
        );
        assert!(
            registry.remove_local_room_member(room, id(2)),
            "the last member leaving means the replica can unsubscribe"
        );
    }

    #[tokio::test]
    async fn adding_the_same_user_twice_does_not_double_count_the_room() {
        // A reconnect re-joins; without this the room would report the user twice and
        // deliver every message to them twice.
        let registry = registry();
        let room = id(100);

        assert!(registry.add_local_room_member(room, id(1)));
        assert!(!registry.add_local_room_member(room, id(1)));

        assert_eq!(registry.room_members(room).len(), 1);
    }

    #[tokio::test]
    async fn clearing_a_room_reports_who_was_in_it() {
        // The caller needs the membership to notify or clean up per-user state.
        let registry = registry();
        let room = id(100);

        assert!(registry.add_local_room_member(room, id(1)));
        assert!(!registry.add_local_room_member(room, id(2)));

        let mut cleared = registry.clear_local_room(room);
        cleared.sort_unstable();
        assert_eq!(cleared, vec![id(1), id(2)]);
        assert!(registry.room_members(room).is_empty());

        // Clearing again is harmless: a replica may learn of the end twice.
        assert!(registry.clear_local_room(room).is_empty());
    }

    #[tokio::test]
    async fn rooms_are_tracked_per_broadcast() {
        // Membership in one room must not leak into another.
        let registry = registry();
        let a = id(100);
        let b = id(101);

        assert!(registry.add_local_room_member(a, id(1)));
        assert!(registry.room_members(b).is_empty());
        assert_eq!(registry.room_members(a), vec![id(1)]);
    }

    #[tokio::test]
    async fn removing_a_member_of_an_unknown_room_is_a_no_op_not_a_panic() {
        let registry = registry();
        assert!(!registry.remove_local_room_member(id(999), id(1)));
    }

    #[tokio::test]
    async fn room_delivery_reaches_every_member_on_this_replica() {
        let registry = registry();
        let room = id(100);
        let a = id(1);
        let b = id(2);

        let (tx_a, mut rx_a) = channel();
        let (tx_b, mut rx_b) = channel();
        registry.register(a, tx_a).expect("a");
        registry.register(b, tx_b).expect("b");
        assert!(registry.add_local_room_member(room, a));
        assert!(!registry.add_local_room_member(room, b));

        let delivered = registry
            .send_to_room(room, WsPayload::host_disconnected(room, 120, 1))
            .await;

        assert_eq!(delivered, 2);
        assert_eq!(
            rx_a.recv().await.expect("a").event,
            WsEvent::HostDisconnected
        );
        assert_eq!(
            rx_b.recv().await.expect("b").event,
            WsEvent::HostDisconnected
        );
    }

    #[tokio::test]
    async fn room_delivery_skips_members_without_a_local_socket() {
        // A member can leave the SET before the socket closes. Iterating members
        // rather than sockets must not stall on the missing ones.
        let registry = registry();
        let room = id(100);
        let connected = id(1);
        let gone = id(2);

        let (tx_gone, mut rx_gone) = channel();
        let (tx_connected, mut rx_connected) = channel();
        registry.register(gone, tx_gone).expect("gone");
        registry
            .register(connected, tx_connected)
            .expect("connected");
        assert!(registry.add_local_room_member(room, gone));
        assert!(!registry.add_local_room_member(room, connected));

        // `gone` disconnects without leaving the room — the stale-membership case.
        registry.unregister(gone, 0);

        registry
            .send_to_room(room, WsPayload::host_reconnected(room))
            .await;

        assert!(rx_connected.recv().await.is_some());
        assert!(
            rx_gone.try_recv().is_err(),
            "a disconnected socket must not receive"
        );
    }

    #[tokio::test]
    async fn room_delivery_to_an_unknown_room_delivers_nothing() {
        let registry = registry();
        assert_eq!(
            registry
                .send_to_room(id(404), WsPayload::host_reconnected(id(404)))
                .await,
            0
        );
    }

    #[tokio::test]
    async fn user_delivery_fans_out_to_all_of_that_users_sockets() {
        let registry = registry();
        let user = id(1);

        let (tx_web, mut rx_web) = channel();
        let (tx_mobile, mut rx_mobile) = channel();
        registry.register(user, tx_web).expect("web");
        registry.register(user, tx_mobile).expect("mobile");

        assert!(
            registry
                .send_to_sockets(user, WsPayload::host_reconnected(id(9)))
                .await
        );

        assert!(rx_web.recv().await.is_some(), "web must receive");
        assert!(rx_mobile.recv().await.is_some(), "mobile must receive");
    }

    #[tokio::test]
    async fn delivering_to_an_absent_user_reports_false_so_the_caller_can_buffer() {
        // The bool is what keeps buffering out of the I/O-free registry.
        let registry = registry();
        assert!(
            !registry
                .send_to_sockets(id(1), WsPayload::host_reconnected(id(9)))
                .await,
            "an absent user must be distinguishable from a present one"
        );
    }

    #[tokio::test]
    async fn multi_user_delivery_names_exactly_who_is_absent() {
        let registry = registry();
        let present = id(1);
        let absent = id(2);

        let (tx_present, mut rx_present) = channel();
        registry.register(present, tx_present).expect("present");

        let missing = registry
            .send_to_sockets_many(&[present, absent], WsPayload::host_reconnected(id(9)))
            .await;

        assert_eq!(missing, vec![absent]);
        assert!(rx_present.recv().await.is_some());
    }

    #[tokio::test]
    async fn broadcast_all_reaches_every_connected_user() {
        let registry = registry();
        let users: Vec<Uuid> = (1..=4).map(id).collect();

        let mut receivers = Vec::new();
        for user in &users {
            let (tx, rx) = channel();
            registry.register(*user, tx).expect("register");
            receivers.push(rx);
        }

        let delivered = registry.broadcast_all(WsPayload::home_invalidated()).await;
        assert_eq!(delivered, users.len());

        for mut rx in receivers {
            assert_eq!(
                rx.recv().await.expect("payload").event,
                WsEvent::HomeInvalidated
            );
        }
    }

    #[tokio::test]
    async fn broadcast_all_with_nobody_connected_delivers_nothing() {
        let registry = registry();
        assert_eq!(
            registry.broadcast_all(WsPayload::home_invalidated()).await,
            0
        );
    }

    #[tokio::test]
    async fn a_send_to_a_closed_socket_does_not_panic_or_block() {
        // The receiver is dropped, so the socket is gone. Delivery must log and move
        // on rather than propagate an error the caller cannot act on.
        let registry = registry();
        let user = id(1);

        let (tx, rx) = channel();
        registry.register(user, tx).expect("register");
        drop(rx);

        registry
            .send_to_sockets(user, WsPayload::host_reconnected(id(9)))
            .await;
        registry.broadcast_all(WsPayload::home_invalidated()).await;
        registry
            .send_to_room(id(1), WsPayload::host_reconnected(id(9)))
            .await;
    }

    #[tokio::test]
    async fn shutdown_announces_itself_as_recoverable_and_roomless() {
        // A client told the server is going away must reconnect rather than conclude
        // the broadcast ended — which is exactly why this is not a plain room error.
        let registry = registry();
        let user = id(1);
        let (tx, mut rx) = channel();
        registry.register(user, tx).expect("register");

        registry.broadcast_all(WsPayload::server_shutdown()).await;

        let payload = rx.recv().await.expect("payload");
        let body = serde_json::to_value(&payload.data).expect("serialise");
        assert_eq!(body["code"], "SERVER_SHUTDOWN");
        assert_eq!(body["recoverable"], true);
        assert!(body.get("broadcastId").is_none());
    }

    #[test]
    fn the_buffer_is_bounded_in_both_depth_and_age() {
        // §9.4: every Redis key has a TTL, and the buffer is bounded because it
        // shares the memory-capped instance with everything else.
        // Compile-time constants, so the bound is checked at compile time.
        const {
            assert!(MESSAGE_BUFFER_TTL_SECS > 0, "a zero TTL never expires");
            assert!(
                OFFLINE_MESSAGE_HISTORY < MESSAGE_BUFFER_SIZE,
                "the history bound is the real one; the channel is just back pressure"
            );
        }
    }

    #[test]
    fn the_channel_capacity_is_positive() {
        // A zero-capacity channel would make every send await a receiver, turning a
        // dropped socket into a stalled task.
        const {
            assert!(MESSAGE_BUFFER_SIZE > 0);
            assert!(MESSAGE_BUFFER_TTL_SECS > 0);
        }
    }
}
