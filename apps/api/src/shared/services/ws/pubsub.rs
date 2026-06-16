use crate::config::MenoConfig;
use crate::modules::chat::errors::ChatError;
use crate::shared::services::redis::RedisService;
use crate::shared::services::redis::keys::RedisKey;
use crate::shared::services::ws::WsService;
use crate::shared::services::ws::dto::WsPayload;
use anyhow::Result;
use fred::clients::SubscriberClient;
use fred::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

/// Main Redis channel for targeted messages (user DMs, system broadcasts)
const WS_MAIN_CHANNEL: &str = "meno:ws:events";

/// Redis channel prefix for room-based messages
/// Format: meno:ws:room:{broadcast_id}
const WS_ROOM_CHANNEL_PREFIX: &str = "meno:ws:room:";

/// TTL for room memberships (1 hour)
const ROOM_MEMBERSHIP_TTL_SECS: u64 = 3600;

/// Maximum retry attempts for pub/sub operations
const MAX_RETRIES: u32 = 3;

/// Envelope for room-based delivery
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsRoomEnvelope {
    pub room_id: Uuid,
    pub payload: WsPayload,
}

/// Envelope for targeted delivery (still useful for DMs)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsUserEnvelope {
    pub user_id: Uuid,
    pub payload: WsPayload,
}

/// Broadcast-to-all envelope (no target)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsBroadcastEnvelope {
    pub payload: WsPayload,
}

/// Unified envelope type with explicit tagging for serialization
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WsPubSubEnvelope {
    #[serde(rename = "room")]
    Room(WsRoomEnvelope),

    #[serde(rename = "user")]
    User(WsUserEnvelope),

    #[serde(rename = "broadcast")]
    Broadcast(WsBroadcastEnvelope),
}
impl WsPubSubEnvelope {
    /// Create a room envelope
    pub fn room(room_id: Uuid, payload: WsPayload) -> Self {
        Self::Room(WsRoomEnvelope { room_id, payload })
    }

    /// Create a user envelope
    pub fn user(user_id: Uuid, payload: WsPayload) -> Self {
        Self::User(WsUserEnvelope { user_id, payload })
    }

    /// Create a broadcast envelope
    pub fn broadcast(payload: WsPayload) -> Self {
        Self::Broadcast(WsBroadcastEnvelope { payload })
    }
}

/// Owned by AppState. Wraps a fred SubscriberClient for the receive loop
/// and a regular Pool for publishing.
/// Owns both the publisher client (a regular fred Client for PUBLISH calls)
/// and the subscriber client (a dedicated SubscriberClient that holds the
/// persistent SUBSCRIBE connection).
///
/// Both are built from the same Builder so they share config and TLS settings
/// but maintain completely independent TCP connections — exactly what Redis
/// requires (a connection in SUBSCRIBE mode cannot issue PUBLISH).
#[derive(Clone)]
pub struct WsPubSubBridge {
    /// Regular Redis client for publishing
    publisher: Client,

    /// Dedicated subscriber client for receiving messages
    subscriber: Arc<SubscriberClient>,

    /// Local WebSocket service for delivery
    hub: WsService,

    /// Redis service for room management
    redis: RedisService,
}
impl WsPubSubBridge {
    pub async fn build(config: &MenoConfig, hub: WsService, redis: RedisService) -> Result<Self> {
        let builder = Builder::from_config(Config::from_url(&config.redis_url)?);

        let publisher: Client = builder.build()?;
        publisher.init().await?;

        let subscriber: SubscriberClient = builder.build_subscriber_client()?;
        subscriber.init().await?;

        tracing::info!("WsPubSubBridge clients initialised");

        Ok(Self {
            publisher,
            subscriber: Arc::new(subscriber),
            hub,
            redis,
        })
    }

    /// Publish a message to all members of a room
    #[tracing::instrument(
        name = "pubsub.publish_to_room",
        skip(self, payload),
        fields(broadcast_id = %broadcast_id, event = %payload.event)
    )]
    pub async fn publish_to_room(&self, broadcast_id: Uuid, payload: WsPayload) {
        let envelope = WsPubSubEnvelope::room(broadcast_id, payload);
        let channel = format!("{}{}", WS_ROOM_CHANNEL_PREFIX, broadcast_id);
        self.publish(&channel, envelope, MAX_RETRIES).await;
    }

    /// Publish a message to a specific user
    #[tracing::instrument(
        name = "pubsub.publish_to_user",
        skip(self, payload),
        fields(target_user_id = %user_id)
    )]
    pub async fn publish_to_user(&self, user_id: Uuid, payload: WsPayload) {
        let envelope = WsPubSubEnvelope::user(user_id, payload);
        self.publish(WS_MAIN_CHANNEL, envelope, MAX_RETRIES).await;
    }

    /// Publish to multiple users efficiently
    #[tracing::instrument(
        name = "pubsub.publish_to_users",
        skip(self, payload),
        fields(count = user_ids.len())
    )]
    pub async fn publish_to_users(&self, user_ids: &[Uuid], payload: WsPayload) {
        if user_ids.is_empty() {
            return;
        }

        // For small batches (< 10), publish individually
        if user_ids.len() <= 10 {
            for &user_id in user_ids {
                self.publish_to_user(user_id, payload.clone()).await;
            }
            return;
        }

        for &user_id in user_ids {
            let envelope = WsPubSubEnvelope::user(user_id, payload.clone());
            if let Ok(json) = serde_json::to_string(&envelope) {
                let _ = self.redis.publish(WS_MAIN_CHANNEL, json).await;
            }
        }
    }

    /// Broadcast to all connected users across all instances
    #[tracing::instrument(name = "pubsub.broadcast_all", skip(self, payload))]
    pub async fn broadcast_all(&self, payload: WsPayload) {
        let envelope = WsPubSubEnvelope::broadcast(payload);
        self.publish(WS_MAIN_CHANNEL, envelope, MAX_RETRIES).await;
    }

    /// Join a user to a room
    #[tracing::instrument(
        name = "pubsub.join_room",
        skip(self),
        fields(user_id = %user_id, broadcast_id = %broadcast_id)
    )]
    pub async fn join_room(&self, user_id: Uuid, broadcast_id: Uuid) -> Result<(), ChatError> {
        let room_key = RedisKey::new(format!("room:{}:members", broadcast_id));
        let ttl = ROOM_MEMBERSHIP_TTL_SECS as i64;
        let user_str = user_id.to_string();

        // Add user to room set
        let added = self.redis.sadd::<i64, _>(&room_key, &user_str).await?;

        // Set TTL on room (extends existing TTL)
        let _: () = self.redis.expire(&room_key, ttl).await?;

        // Subscribe this instance to the room channel
        let channel = format!("{}{}", WS_ROOM_CHANNEL_PREFIX, broadcast_id);
        if let Err(e) = self.subscriber.subscribe(&channel).await {
            tracing::warn!(
                error = %e,
                room = %broadcast_id,
                "Failed to subscribe local instance to room"
            );
            // Non-fatal - local delivery will still work
        }

        tracing::debug!(
            user_id = %user_id,
            broadcast_id = %broadcast_id,
            "User joined room (added={})",
            added
        );

        Ok(())
    }

    /// Remove a user from a room
    #[tracing::instrument(
        name = "pubsub.leave_room",
        skip(self),
        fields(user_id = %user_id, broadcast_id = %broadcast_id)
    )]
    pub async fn leave_room(&self, user_id: Uuid, broadcast_id: Uuid) -> Result<(), ChatError> {
        let room_key = RedisKey::new(format!("room:{}:members", broadcast_id));
        let user_str = user_id.to_string();

        // Remove user from room set
        let removed = self.redis.srem::<i64, _>(&room_key, &user_str).await?;

        // Check if room is empty
        let count = self.redis.scard::<i64>(&room_key).await?;
        if count == 0 {
            // Unsubscribe local instance from room
            let channel = format!("{}{}", WS_ROOM_CHANNEL_PREFIX, broadcast_id);
            if let Err(e) = self.subscriber.unsubscribe(&channel).await {
                tracing::warn!(
                    error = %e,
                    room = %broadcast_id,
                    "Failed to unsubscribe from empty room"
                );
            }
            // Clean up Redis key
            let _ = self.redis.del(&room_key).await?;
        }

        tracing::debug!(
            user_id = %user_id,
            broadcast_id = %broadcast_id,
            "User left room (removed={})",
            removed
        );

        Ok(())
    }

    /// Check if a user is in a room
    #[tracing::instrument(
        name = "pubsub.is_member",
        skip(self),
        fields(user_id = %user_id, broadcast_id = %broadcast_id)
    )]
    pub async fn is_member(&self, broadcast_id: Uuid, user_id: Uuid) -> Result<bool, ChatError> {
        let key = RedisKey::new(format!("room:{}:members", broadcast_id));
        let user_str = user_id.to_string();

        let is_member = self.redis.sismember::<bool, _>(&key, &user_str).await?;
        Ok(is_member)
    }

    /// Get all members of a room
    #[tracing::instrument(
        name = "pubsub.get_room_members",
        skip(self),
        fields(broadcast_id = %broadcast_id)
    )]
    pub async fn get_room_members(&self, broadcast_id: Uuid) -> Result<Vec<Uuid>, ChatError> {
        let key = RedisKey::new(format!("room:{}:members", broadcast_id));

        let members = self.redis.smembers::<Vec<String>>(&key).await?;

        let uuids: Vec<Uuid> = members
            .into_iter()
            .filter_map(|s| Uuid::parse_str(&s).ok())
            .collect();

        tracing::debug!(
            broadcast_id = %broadcast_id,
            count = uuids.len(),
            "Retrieved room members"
        );

        Ok(uuids)
    }

    /// Get count of members in a room
    pub async fn room_member_count(&self, broadcast_id: Uuid) -> Result<i64, ChatError> {
        let key = RedisKey::new(format!("room:{}:members", broadcast_id));
        let count = self.redis.scard::<i64>(&key).await?;
        Ok(count)
    }

    /// Initialize a room with a list of participants (for broadcast start)
    #[tracing::instrument(
        name = "pubsub.init_room",
        skip(self, participant_ids),
        fields(broadcast_id = %broadcast_id, count = participant_ids.len())
    )]
    pub async fn init_room(
        &self,
        broadcast_id: Uuid,
        participant_ids: &[Uuid],
    ) -> Result<(), ChatError> {
        if participant_ids.is_empty() {
            return Ok(());
        }

        let key = RedisKey::new(format!("room:{}:members", broadcast_id));
        let ttl = ROOM_MEMBERSHIP_TTL_SECS as i64;

        // Convert Uuids to strings
        let ids: Vec<String> = participant_ids.iter().map(|id| id.to_string()).collect();

        // Add all participants at once
        let added = self.redis.sadd::<i64, Vec<String>>(&key, ids).await?;
        let _: () = self.redis.expire(&key, ttl).await?;

        // Subscribe local instance to room
        let channel = format!("{}{}", WS_ROOM_CHANNEL_PREFIX, broadcast_id);
        if let Err(e) = self.subscriber.subscribe(&channel).await {
            tracing::warn!(
                error = %e,
                room = %broadcast_id,
                "Failed to subscribe to initialized room"
            );
        }

        tracing::info!(
            broadcast_id = %broadcast_id,
            added = added,
            total = participant_ids.len(),
            "Room initialized"
        );

        Ok(())
    }

    /// Spawn the subscriber receive loop
    pub fn spawn_subscriber_loop(&self) {
        let subscriber = Arc::clone(&self.subscriber);
        let hub = self.hub.clone();

        // Subscribe to main channel for user/broadcast messages
        let main_channel = WS_MAIN_CHANNEL.to_string();
        let subscriber_clone = subscriber.clone();

        tokio::spawn(async move {
            if let Err(e) = subscriber_clone.subscribe(&main_channel).await {
                tracing::error!(
                    error = %e,
                    channel = %main_channel,
                    "Failed to subscribe to main channel"
                );
                return;
            }
            tracing::info!(channel = %main_channel, "Subscribed to main channel");
        });

        // Subscribe to room pattern
        let pattern = format!("{}*", WS_ROOM_CHANNEL_PREFIX);
        let subscriber_clone = subscriber.clone();

        tokio::spawn(async move {
            if let Err(e) = subscriber_clone.psubscribe(&pattern).await {
                tracing::error!(
                    error = %e,
                    pattern = %pattern,
                    "Failed to subscribe to room pattern"
                );
                return;
            }
            tracing::info!(pattern = %pattern, "Subscribed to room pattern");
        });

        // Run the main "receive" loop
        tokio::spawn(async move {
            run_subscriber_loop(subscriber, hub).await;
        });
    }

    /// Clean up empty rooms (maintenance)
    #[tracing::instrument(name = "pubsub.cleanup_rooms", skip(self))]
    pub async fn cleanup_empty_rooms(&self) -> Result<u64, ChatError> {
        let pattern = "room:*:members";
        let cleaned = self.redis.delete_by_pattern(pattern).await?;

        if cleaned > 0 {
            tracing::info!(cleaned = cleaned, "Cleaned up empty rooms");
        }

        Ok(cleaned)
    }

    /// Publish with retry logic
    async fn publish(&self, channel: &str, envelope: WsPubSubEnvelope, max_retries: u32) {
        let json = match serde_json::to_string(&envelope) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!(error = %e, channel = %channel, "Failed to serialize envelope");
                return;
            }
        };

        let mut attempts = 0;
        loop {
            attempts += 1;
            match self.publisher.publish::<i64, _, _>(channel, &json).await {
                Ok(_) => {
                    if attempts > 1 {
                        tracing::debug!(
                            channel = %channel,
                            attempts = attempts,
                            "Publish succeeded after retry"
                        );
                    }
                    return;
                }
                Err(e) => {
                    if attempts >= max_retries {
                        tracing::error!(
                            error = %e,
                            channel = %channel,
                            attempts = attempts,
                            "Publish failed after all retries"
                        );
                        return;
                    }

                    let backoff = std::time::Duration::from_millis(100 * attempts as u64);
                    tracing::warn!(
                        error = %e,
                        channel = %channel,
                        attempt = attempts,
                        next_retry_ms = backoff.as_millis(),
                        "Publish failed, retrying"
                    );
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }
}

/// Runs inside a dedicated Tokio task. Subscribes to the pub/sub channel and
/// forwards each envelope to the local WsHub.
///
/// Key behaviors:
///  - `manage_subscriptions()` spawns fred's built-in reconnect task, which automatically
///     re-SUBSCRIBEs after any Redis disconnect. We do not need to manually handle
///     reconnections.
///  - `message.rx()` returns `BroadcastReceiver<Message>`. If the task falls behind
///     (Lagged error), some messages are dropped. This is acceptable for WS events because the
///     client will reconcile on reconnect via the Redis buffer drain.
async fn run_subscriber_loop(subscriber: Arc<SubscriberClient>, hub: WsService) {
    // Raw broadcast receiver — no extra task, messages arrive here directly.
    let mut message_rx = subscriber.message_rx();
    loop {
        match message_rx.recv().await {
            Ok(msg) => {
                // Convert the `msg.value` from `fred` to `&str`
                let json = match msg.value.as_str() {
                    Some(s) => s.to_owned(),
                    None => {
                        tracing::warn!(
                            channel = %msg.channel,
                            "Received non-string pub/sub message. Skipping"
                        );
                        continue;
                    }
                };
                let envelope = match serde_json::from_str::<WsPubSubEnvelope>(&json) {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            raw   = %json,
                            "Failed to deserialize WS pub/sub envelope. Skipping"
                        );
                        continue;
                    }
                };
                deliver_locally(&hub, envelope).await;
            }
            Err(RecvError::Lagged(skipped)) => {
                // The receiver fell behind the sender ring-buffer. Some events
                // were dropped. Log and continue — clients reconcile on reconnect.
                tracing::warn!(
                    skipped = skipped,
                    "WS pub/sub receiver lagged. Some events dropped"
                );
            }
            Err(RecvError::Closed) => {
                // The SubscriberClient was dropped or the process is shutting down.
                tracing::info!("WS pub/sub message channel closed. Subscriber loop exiting");
                break;
            }
        }
    }
    tracing::warn!("Subscriber loop exited");
}

/// Deliver an envelope to whichever clients are on THIS instance.
/// The local WsHub handles buffering for offline users automatically.
async fn deliver_locally(hub: &WsService, envelope: WsPubSubEnvelope) {
    match envelope {
        WsPubSubEnvelope::Room(room_env) => {
            // Deliver to all local users in this room
            hub.send_to_room(room_env.room_id, room_env.payload).await;
        }
        WsPubSubEnvelope::User(user_env) => {
            // Deliver to specific user
            hub.send_to_user(user_env.user_id, user_env.payload).await;
        }
        WsPubSubEnvelope::Broadcast(broadcast_env) => {
            // Deliver to all local users
            hub.broadcast_all(broadcast_env.payload).await;
        }
    }
}
