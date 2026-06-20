use crate::shared::constants::{
    MAX_WS_CONNECTIONS_PER_USER, MESSAGE_BUFFER_SIZE, MESSAGE_BUFFER_TTL_SECS,
};
use crate::shared::services::redis::Redis;
use crate::shared::services::redis::keys::RedisKey;
use crate::shared::services::ws::dto::{WsErrorCode, WsPayload};
use crate::shared::services::ws::model::WsEvent;
use crate::shared::services::ws::pubsub::WsPubSubBridge;
use dashmap::{DashMap, DashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

pub mod dto;
pub mod errors;
pub mod handlers;
pub mod model;
pub mod pubsub;

/// A single WebSocket connection for one user.
/// A user may have multiple concurrent connections (e.g., web + mobile),
/// each identified by a unique `conn_id`.
#[derive(Clone)]
pub struct ConnectionSender {
    pub conn_id: usize,
    sender: mpsc::Sender<Arc<WsPayload>>,
}

/// Manages all in-process WebSocket connections and provides the local
/// delivery layer for the pub/sub bridge.
///
/// ## Responsibilities
/// - **Registry** — tracks which users have active connections on *this*
///   instance (`clients` map).
/// - **Room tracking** — maintains a local set of which users are in each
///   broadcast room, mirroring the Redis membership set.
/// - **Delivery** — sends payloads to local connections; buffers to Redis
///   for offline users.
/// - **Pub/sub integration** — `join_room` / `leave_room` delegate to
///   `WsPubSubBridge` to keep the Redis membership set consistent.
///
/// ## What it does NOT do
/// Cross-instance delivery.  That is the exclusive responsibility of
/// `WsPubSubBridge`.  Application code that needs to reach users on other
/// instances must go through the bridge, not this service.
#[derive(Clone)]
pub struct WsService {
    /// `user_id` → all active connections for that user on this instance.
    clients: Arc<DashMap<Uuid, Vec<ConnectionSender>>>,

    /// `broadcast_id` → set of `user_id`s currently in that room on this instance.
    /// Mirrors the Redis membership set for local fast-path delivery.
    rooms: Arc<DashMap<Uuid, DashSet<Uuid>>>,

    /// Monotonically increasing connection ID source.
    conn_seq: Arc<AtomicUsize>,

    redis: Redis,
}

impl WsService {
    #[must_use]
    pub fn new(redis: Redis) -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            rooms: Arc::new(DashMap::new()),
            conn_seq: Arc::new(AtomicUsize::new(0)),
            redis,
        }
    }

    /// Register a new WebSocket connection for `user_id`.
    ///
    /// Returns the unique `conn_id` for later unregistration, or `None` if
    /// the per-user connection limit has been reached.
    pub fn register(&self, user_id: Uuid, sender: mpsc::Sender<Arc<WsPayload>>) -> Option<usize> {
        let mut entry = self.clients.entry(user_id).or_default();

        if entry.len() >= MAX_WS_CONNECTIONS_PER_USER {
            tracing::warn!(
                user_id     = %user_id,
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

    /// Unregister one specific connection.
    ///
    /// If this was the last connection for the user, the client entry is
    /// removed entirely, so `is_online` returns `false`.
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

    /// Add a user to a broadcast room on this instance **and** in Redis.
    ///
    /// Call this from two places:
    /// 1. `BroadcastService::join` (HTTP) — when a user joins a live broadcast.
    /// 2. `ws/handlers::handle_socket` — when a user reconnects via WebSocket
    ///    while they are still an active participant.
    ///
    /// It is safe to call multiple times; Redis SADD and the local `DashSet` are
    /// both idempotent.
    pub async fn join_room(&self, user_id: Uuid, broadcast_id: Uuid, bridge: &WsPubSubBridge) {
        // Update the local room map.
        self.rooms.entry(broadcast_id).or_default().insert(user_id);

        // Update Redis and subscribe this instance to the room channel.
        if let Err(e) = bridge.join_room(user_id, broadcast_id).await {
            tracing::warn!(
                error        = %e,
                user_id      = %user_id,
                broadcast_id = %broadcast_id,
                "Failed to join room in Redis — local delivery still works"
            );
        }
    }

    /// Remove a user from a broadcast room on this instance **and** in Redis.
    ///
    /// Call this from:
    /// 1. `BroadcastService::leave` (HTTP leave endpoint).
    /// 2. `BroadcastService::end` (broadcast ended — all participants removed).
    /// 3. `ws/handlers::handle_socket` cleanup on disconnect.
    pub async fn leave_room(&self, user_id: Uuid, broadcast_id: Uuid, bridge: &WsPubSubBridge) {
        if let Some(room) = self.rooms.get(&broadcast_id) {
            room.remove(&user_id);
            if room.is_empty() {
                drop(room);
                self.rooms.remove(&broadcast_id);
            }
        }

        if let Err(e) = bridge.leave_room(user_id, broadcast_id).await {
            tracing::warn!(
                error        = %e,
                user_id      = %user_id,
                broadcast_id = %broadcast_id,
                "Failed to leave room in Redis"
            );
        }
    }

    /// Deliver a payload to all local connections in a room.
    ///
    /// This is called by `deliver_locally` in `pubsub.rs` after a message
    /// arrives from Redis.  It only touches the local `rooms` `DashMap` and
    /// never calls Redis itself — the bridge already handled cross-instance
    /// routing.
    pub async fn send_to_room(&self, broadcast_id: Uuid, payload: WsPayload) {
        let Some(room) = self.rooms.get(&broadcast_id) else {
            return;
        };

        let arc_payload = Arc::new(payload);
        for user_id in room.iter() {
            if let Some(senders) = self.clients.get(&*user_id) {
                for sender in senders.iter() {
                    let _ = sender.sender.send(Arc::clone(&arc_payload)).await;
                }
            }
        }
    }

    /// Returns `true` if this was the *first* local member of the room
    /// (i.e. this instance needs to subscribe to the room channel).
    #[must_use]
    pub fn add_local_room_member(&self, broadcast_id: Uuid, user_id: Uuid) -> bool {
        let room = self.rooms.entry(broadcast_id).or_default();
        let was_empty = room.is_empty();
        room.insert(user_id);
        was_empty
    }

    /// Returns `true` if this was the *last* local member (this instance
    /// should unsubscribe from the room channel).
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

    /// Drops every local member of a room at once — used when this instance
    /// learns the broadcast has ended.
    #[must_use]
    pub fn clear_local_room(&self, broadcast_id: Uuid) -> Vec<Uuid> {
        self.rooms
            .remove(&broadcast_id)
            .map(|(_, set)| set.iter().map(|u| *u).collect())
            .unwrap_or_default()
    }

    /// Deliver a payload to all connections for a specific user on this instance.
    ///
    /// If the user is not connected locally, the message is buffered in Redis
    /// so it can be replayed when they reconnect.
    pub async fn send_to_user(&self, user_id: Uuid, payload: WsPayload) {
        if let Some(senders) = self.clients.get(&user_id) {
            let arc = Arc::new(payload);
            for s in senders.iter() {
                let _ = s.sender.send(Arc::clone(&arc)).await;
            }
            return;
        }

        // User is not connected on this instance — buffer for later replay.
        self.buffer_message(user_id, payload).await;
    }

    /// Deliver a payload to multiple users (local delivery only).
    ///
    /// For cross-instance fan-out use `WsPubSubBridge::publish_to_users`.
    /// This variant is kept for cases where the caller has already resolved
    /// that all target users are local (e.g., inside `deliver_locally`).
    pub async fn send_to_users(&self, user_ids: &[Uuid], payload: WsPayload) {
        let arc = Arc::new(payload);
        for &uid in user_ids {
            if let Some(senders) = self.clients.get(&uid) {
                for s in senders.iter() {
                    let _ = s.sender.send(Arc::clone(&arc)).await;
                }
            } else {
                self.buffer_message(uid, (*arc).clone()).await;
            }
        }
    }

    /// Deliver a payload to every locally connected user.
    pub async fn broadcast_all(&self, payload: WsPayload) {
        let arc = Arc::new(payload);
        for entry in self.clients.iter() {
            for s in entry.value() {
                let _ = s.sender.send(Arc::clone(&arc)).await;
            }
        }
    }

    /// Send a structured error payload to a specific user.
    pub async fn send_error(
        &self,
        user_id: Uuid,
        broadcast_id: Uuid,
        code: WsErrorCode,
        message: impl Into<String>,
    ) {
        let payload = WsPayload::error(broadcast_id, code, message);
        self.send_to_user(user_id, payload).await;
    }

    pub async fn send_unsupported_error(&self, user_id: Uuid, message: String) {
        self.send_error(user_id, Uuid::nil(), WsErrorCode::Unsupported, message)
            .await;
    }

    /// Store a message for an offline user in a Redis ring-buffer.
    ///
    /// Uses `LPUSH` + `LTRIM` to keep only the last `MESSAGE_BUFFER_SIZE`
    /// messages, capped at `MESSAGE_BUFFER_TTL_SECS`.
    pub async fn buffer_message(&self, user_id: Uuid, payload: WsPayload) {
        let key = RedisKey::ws_buffer(user_id);
        let json = match serde_json::to_string(&payload) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!(error = %e, user_id = %user_id, "Failed to serialise buffered message");
                return;
            }
        };

        let _ = self.redis.lpush(&key, &json).await;
        let _ = self.redis.ltrim(&key, 0, MESSAGE_BUFFER_SIZE).await;
        let _ = self.redis.expire(&key, MESSAGE_BUFFER_TTL_SECS).await;
    }

    /// Drain the offline message buffer for a user and return messages in
    /// chronological order (oldest first).
    ///
    /// Uses a Lua script for an atomic LRANGE + DEL so no message is delivered
    /// twice if two connections race to reconnect.
    pub async fn drain_message_buffer(&self, user_id: Uuid) -> Vec<WsPayload> {
        let key = RedisKey::ws_buffer(user_id).to_string();

        let script = r"
            local items = redis.call('LRANGE', KEYS[1], 0, -1)
            redis.call('DEL', KEYS[1])
            return items
        ";

        let items: Vec<String> = match self
            .redis
            .eval::<Vec<String>, Vec<String>, Vec<String>>(script, vec![key], vec![])
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, user_id = %user_id, "Failed to drain message buffer");
                return Vec::new();
            }
        };

        // Items were LPUSH-ed (newest first); reverse to get chronological order.
        items
            .into_iter()
            .rev()
            .filter_map(|json| serde_json::from_str::<WsPayload>(&json).ok())
            .collect()
    }

    #[must_use]
    pub fn is_online(&self, user_id: Uuid) -> bool {
        self.clients.contains_key(&user_id)
    }

    #[must_use]
    pub fn connection_count(&self, user_id: Uuid) -> usize {
        self.clients.get(&user_id).map_or(0, |v| v.len())
    }

    /// Return all user IDs that have at least one active connection on this
    /// instance.  Used for "now-live" fan-outs where the caller wants to
    /// notify every online user.
    #[must_use]
    pub fn get_online_users(&self) -> Vec<Uuid> {
        self.clients.iter().map(|e| *e.key()).collect()
    }

    /// Notify all connected clients of an imminent shutdown so they can
    /// trigger their reconnection logic before the process exits.
    pub async fn close_all_connections(&self) {
        let payload = WsPayload::new(
            WsEvent::BroadcastError,
            serde_json::json!({ "code": "SERVER_SHUTDOWN", "recoverable": true }),
        );
        self.broadcast_all(payload).await;

        // Give clients a short window to acknowledge before the hard close.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
