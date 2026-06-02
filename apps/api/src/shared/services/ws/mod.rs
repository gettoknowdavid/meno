use crate::shared::constants::{
    MAX_WS_CONNECTIONS_PER_USER, MESSAGE_BUFFER_SIZE, MESSAGE_BUFFER_TTL_SECS,
};
use crate::shared::services::redis::RedisService;
use crate::shared::services::redis::keys::RedisKey;
use crate::shared::services::ws::dto::{WsErrorCode, WsPayload};
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::mpsc;
use uuid::Uuid;

pub mod dto;
pub mod errors;
pub mod handlers;
pub mod model;

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
    conn_seq: Arc<AtomicUsize>,
    redis: RedisService,
}
impl WsService {
    pub fn new(redis: RedisService) -> Self {
        let clients = Arc::new(DashMap::new());
        let conn_seq = Arc::new(AtomicUsize::new(0));
        Self {
            clients,
            conn_seq,
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
}
