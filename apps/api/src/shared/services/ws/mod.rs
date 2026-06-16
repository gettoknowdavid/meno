use crate::shared::constants::{
    MAX_WS_CONNECTIONS_PER_USER, MESSAGE_BUFFER_SIZE, MESSAGE_BUFFER_TTL_SECS,
};
use crate::shared::services::redis::RedisService;
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

#[derive(Clone)]
pub struct ConnectionSender {
    conn_id: usize,
    sender: mpsc::Sender<Arc<WsPayload>>,
}

/// WebSocket service managing user connections with offline message buffering
///
/// Design decisions:
/// - Uses DashMap for O(1) concurrent access to the connection map
/// - Offline messages buffered in Redis (survives server restarts)
/// - Ring buffer pattern (LPUSH + LTRIM) for bounded queue
/// - Atomic sequence for connection IDs
#[derive(Clone)]
pub struct WsService {
    clients: Arc<DashMap<Uuid, Vec<ConnectionSender>>>,
    rooms: Arc<DashMap<Uuid, DashSet<Uuid>>>,
    conn_seq: Arc<AtomicUsize>,
    redis: RedisService,
}
impl WsService {
    pub fn new(redis: RedisService) -> Self {
        Self {
            clients: Arc::new(DashMap::new()),
            rooms: Arc::new(DashMap::new()),
            conn_seq: Arc::new(AtomicUsize::new(0)),
            redis,
        }
    }

    /// Register a new connection for a user
    /// Returns the connection ID for later unregistration
    pub fn register(&self, user_id: Uuid, sender: mpsc::Sender<Arc<WsPayload>>) -> Option<usize> {
        let mut entry = self.clients.entry(user_id).or_insert_with(Vec::new);

        if entry.len() >= MAX_WS_CONNECTIONS_PER_USER {
            tracing::warn!(
                user_id = %user_id,
                connections = entry.len(),
                "WS connection limit reached, rejecting"
            );
            return None;
        }

        let conn_id = self.conn_seq.fetch_add(1, Ordering::Relaxed);
        entry.push(ConnectionSender { conn_id, sender });
        Some(conn_id)
    }

    /// Register a user and join their rooms
    pub async fn register_with_rooms(
        &self,
        user_id: Uuid,
        sender: mpsc::Sender<Arc<WsPayload>>,
        broadcast_ids: &[Uuid],
        bridge: &WsPubSubBridge,
    ) -> Option<usize> {
        let conn_id = self.register(user_id, sender)?;

        // Join each room
        for &broadcast_id in broadcast_ids {
            self.join_room(user_id, broadcast_id, bridge).await;
        }

        Some(conn_id)
    }

    /// Join a room (local + Redis)
    pub async fn join_room(&self, user_id: Uuid, broadcast_id: Uuid, bridge: &WsPubSubBridge) {
        // Local room tracking
        self.rooms
            .entry(broadcast_id)
            .or_insert_with(DashSet::new)
            .insert(user_id);

        // Redis membership
        if let Err(e) = bridge.join_room(user_id, broadcast_id).await {
            tracing::warn!(error = %e, "Failed to join room in Redis");
        }
    }

    /// Leave a room
    pub async fn leave_room(&self, user_id: Uuid, broadcast_id: Uuid, bridge: &WsPubSubBridge) {
        if let Some(room) = self.rooms.get(&broadcast_id) {
            room.remove(&user_id);
            if room.is_empty() {
                drop(room);
                self.rooms.remove(&broadcast_id);
            }
        }

        if let Err(e) = bridge.leave_room(user_id, broadcast_id).await {
            tracing::warn!(error = %e, "Failed to leave room in Redis");
        }
    }

    /// Send to all members of a room (local only)
    pub async fn send_to_room(&self, broadcast_id: Uuid, payload: WsPayload) {
        if let Some(room) = self.rooms.get(&broadcast_id) {
            let arc_payload = Arc::new(payload);
            for user_id in room.iter() {
                let user_id = *user_id;
                if let Some(senders) = self.clients.get(&user_id) {
                    for sender in senders.iter() {
                        let _ = sender.sender.send(Arc::clone(&arc_payload)).await;
                    }
                }
            }
        }
    }

    /// Get room members (from the local cache or fallback to Redis)
    pub async fn get_room_members(&self, broadcast_id: Uuid, bridge: &WsPubSubBridge) -> Vec<Uuid> {
        // Try local first
        if let Some(room) = self.rooms.get(&broadcast_id) {
            return room.iter().map(|u| *u).collect();
        }

        // Fallback to Redis
        bridge
            .get_room_members(broadcast_id)
            .await
            .unwrap_or_default()
    }

    /// Unregister a specific connection
    pub fn unregister(&self, user_id: Uuid, conn_id: usize) {
        if let Some(mut entry) = self.clients.get_mut(&user_id) {
            entry.retain(|e| e.conn_id != conn_id);

            if entry.is_empty() {
                drop(entry);
                self.clients.remove(&user_id);
            }
        }
    }

    /// Send payload to a specific user (all their connections)
    /// If the user is offline, buffers the message in Redis
    pub async fn send_to_user(&self, user_id: Uuid, payload: WsPayload) {
        // Check if user is online
        if let Some(senders) = self.clients.get(&user_id) {
            let arc_payload = Arc::new(payload);
            for sender in senders.iter() {
                let _ = sender.sender.send(Arc::clone(&arc_payload)).await;
            }
            return;
        }

        // User is offline - buffer in Redis
        self.buffer_message(user_id, payload).await;
    }

    /// Send payload to multiple users efficiently
    pub async fn send_to_users(&self, user_ids: &[Uuid], payload: WsPayload) {
        let arc_payload = Arc::new(payload);
        for &user_id in user_ids {
            if let Some(senders) = self.clients.get(&user_id) {
                for sender in senders.iter() {
                    let _ = sender.sender.send(Arc::clone(&arc_payload)).await;
                }
            } else {
                // Offline user - buffer
                self.buffer_message(user_id, (*arc_payload).clone()).await;
            }
        }
    }

    /// Buffer a message for an offline user in Redis
    /// Uses the ring buffer pattern: LPUSH + LTRIM to keep last N messages
    pub async fn buffer_message(&self, user_id: Uuid, payload: WsPayload) {
        let key = RedisKey::ws_buffer(user_id);
        let json = match serde_json::to_string(&payload) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("Failed to serialize buffered message: {}", e);
                return;
            }
        };

        // Push the payload to the top of the list in Redis
        let _ = self.redis.lpush(&key, &json).await;

        // Use LTRIM to keep only last MESSAGE_BUFFER_SIZE items (ring buffer)
        let _ = self.redis.ltrim(&key, 0, MESSAGE_BUFFER_SIZE).await;

        // Set expiry time
        let _ = self.redis.expire(&key, MESSAGE_BUFFER_TTL_SECS).await;
    }

    /// Drain and replay buffered messages for a user on reconnection
    /// Returns messages in chronological order (oldest first)
    pub async fn drain_message_buffer(&self, user_id: Uuid) -> Vec<WsPayload> {
        let key = RedisKey::ws_buffer(user_id).to_string();

        // Atomic GETDEL pattern: LRANGE then DEL
        let script = r#"
            local items = redis.call('LRANGE', KEYS[1], 0, -1)
            redis.call('DEL', KEYS[1])
            return items
        "#;
        let items = match self
            .redis
            .eval::<Vec<String>, Vec<String>, Vec<String>>(script, vec![key], vec![])
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Failed to drain message buffer: {}", e);
                return Vec::new();
            }
        };

        // Items are in LIFO order (newest first) due to LPUSH
        // Reverse to get chronological order
        items
            .into_iter()
            .rev()
            .filter_map(|json| serde_json::from_str::<WsPayload>(&json).ok())
            .collect()
    }

    /// Get the number of active connections for a user
    pub fn connection_count(&self, user_id: Uuid) -> usize {
        self.clients.get(&user_id).map(|v| v.len()).unwrap_or(0)
    }

    /// Check if the user has any active connection
    pub fn is_online(&self, user_id: Uuid) -> bool {
        self.clients.contains_key(&user_id)
    }

    /// Send an error to a specific user
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

    /// Broadcast to ALL connected clients
    pub async fn broadcast_all(&self, payload: WsPayload) {
        let arc_payload = Arc::new(payload);
        for entry in self.clients.iter() {
            for sender in entry.value().iter() {
                let _ = sender.sender.send(Arc::clone(&arc_payload)).await;
            }
        }
    }

    /// Get all online user IDs
    pub fn get_online_users(&self) -> Vec<Uuid> {
        self.clients.iter().map(|v| *v.key()).collect()
    }

    pub async fn close_all_connections(&self) {
        // Send a close frame to all connected clients before shutting down.
        // This lets Flutter/Next.js trigger their reconnection logic cleanly.
        let close_payload = WsPayload::new(
            WsEvent::BroadcastError,
            serde_json::json!({ "code": "SERVER_SHUTDOWN", "recoverable": true }),
        );
        self.broadcast_all(close_payload).await;

        // Give clients 2 seconds to acknowledge before hard close
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
