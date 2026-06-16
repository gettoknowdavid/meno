use crate::config::MenoConfig;
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

/// Targeted messages (user DMs, system-level events sent to specific users).
const WS_MAIN_CHANNEL: &str = "meno:ws:events";

/// Per-broadcast room channels. Format: `meno:ws:room:{broadcast_id}`
/// All instances subscribe to this pattern, so a single PUBLISH reaches
/// every node that has a local listener in that room.
const WS_ROOM_CHANNEL_PREFIX: &str = "meno:ws:room:";

/// Redis pattern that matches every room channel at once.
const WS_ROOM_PATTERN: &str = "meno:ws:room:*";

/// TTL applied to the Redis room-membership set each time it is touched.
/// 1 hour is generous — a broadcast will not last longer than this in practice.
const ROOM_MEMBERSHIP_TTL_SECS: i64 = 3600;

/// How many times to retry a failed PUBLISH before giving up.
const MAX_PUBLISH_RETRIES: u32 = 3;

/// Deliver a payload to every local listener in a specific broadcast room.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsRoomEnvelope {
    pub room_id: Uuid,
    pub payload: WsPayload,
}

/// Deliver a payload to a specific user (on whichever instance they are
/// connected to).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsUserEnvelope {
    pub user_id: Uuid,
    pub payload: WsPayload,
}

/// Deliver a payload to every connected user on every instance.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsBroadcastEnvelope {
    pub payload: WsPayload,
}

/// Unified, tagged envelope. The `type` tag lets every instance deserialise
/// the correct variant without additional routing logic.
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
    pub fn room(room_id: Uuid, payload: WsPayload) -> Self {
        Self::Room(WsRoomEnvelope { room_id, payload })
    }

    pub fn user(user_id: Uuid, payload: WsPayload) -> Self {
        Self::User(WsUserEnvelope { user_id, payload })
    }

    pub fn broadcast(payload: WsPayload) -> Self {
        Self::Broadcast(WsBroadcastEnvelope { payload })
    }
}

/// `WsPubSubBridge` is the single point of contact between application logic
/// and the Redis pub/sublayer.
///
/// ## Connection model
/// Two entirely separate Redis connections are maintained:
/// - `publisher`  — a regular pooled client used only for PUBLISH commands.
/// - `subscriber` — a dedicated `SubscriberClient` that holds the persistent
///   SUBSCRIBE / PSUBSCRIBE connection.
///
/// Redis forbids mixing PUBLISH and SUBSCRIBE on the same connection, so this
/// split is mandatory.
///
/// ## Scaling
/// Any number of API instances can run simultaneously. Each instance:
/// 1. Subscribes to `WS_MAIN_CHANNEL` for user-targeted messages.
/// 2. PSubscribes to `WS_ROOM_PATTERN` for room messages.
/// 3. On receiving a message, `deliver_locally` sends it only to clients
///    connected *on that instance* — no cross-instance coordination needed.
#[derive(Clone)]
pub struct WsPubSubBridge {
    publisher: Client,
    subscriber: Arc<SubscriberClient>,
    hub: WsService,
    redis: RedisService,
}
impl WsPubSubBridge {
    /// Build both Redis clients from the same config and verify connectivity.
    pub async fn build(config: &MenoConfig, hub: WsService, redis: RedisService) -> Result<Self> {
        let builder = Builder::from_config(Config::from_url(&config.redis_url)?);

        let publisher: Client = builder.build()?;
        publisher.init().await?;

        let subscriber: SubscriberClient = builder.build_subscriber_client()?;
        subscriber.init().await?;

        tracing::info!("WsPubSubBridge: Redis publisher and subscriber clients initialised");

        Ok(Self {
            publisher,
            subscriber: Arc::new(subscriber),
            hub,
            redis,
        })
    }

    /// Publish a payload to **every participant** in a live broadcast room.
    ///
    /// This is the primary fan-out path for chat messages, reactions, and
    /// participant-count updates.  It makes exactly **one Redis PUBLISH call**
    /// regardless of how many participants are in the room, and zero DB queries.
    #[tracing::instrument(
        name  = "pubsub.publish_to_room",
        skip  (self, payload),
        fields(broadcast_id = %broadcast_id, event = %payload.event)
    )]
    pub async fn publish_to_room(&self, broadcast_id: Uuid, payload: WsPayload) {
        let channel = room_channel(broadcast_id);
        let envelope = WsPubSubEnvelope::room(broadcast_id, payload);
        self.publish(&channel, envelope).await;
    }

    /// Publish a payload to a **single user** on whichever instance they are
    /// connected to.
    #[tracing::instrument(
        name  = "pubsub.publish_to_user",
        skip  (self, payload),
        fields(user_id = %user_id, event = %payload.event)
    )]
    pub async fn publish_to_user(&self, user_id: Uuid, payload: WsPayload) {
        let envelope = WsPubSubEnvelope::user(user_id, payload);
        self.publish(WS_MAIN_CHANNEL, envelope).await;
    }

    /// Publish a payload to **multiple specific users** in one pipeline.
    ///
    /// Uses a Redis pipeline so the entire batch is sent in a single round
    /// trip regardless of the number of users.
    #[tracing::instrument(
        name  = "pubsub.publish_to_users",
        skip  (self, payload),
        fields(count = user_ids.len(), event = %payload.event)
    )]
    pub async fn publish_to_users(&self, user_ids: &[Uuid], payload: WsPayload) {
        if user_ids.is_empty() {
            return;
        }

        // For small sets (≤ 10) the overhead of a pipeline is not worth it.
        if user_ids.len() <= 10 {
            for &uid in user_ids {
                self.publish_to_user(uid, payload.clone()).await;
            }
            return;
        }

        // For larger sets, pipeline all PUBLISH calls into a single round trip.
        let pipeline = self.publisher.pipeline();
        let mut serialisation_errors = 0usize;

        for &uid in user_ids {
            let envelope = WsPubSubEnvelope::user(uid, payload.clone());
            match serde_json::to_string(&envelope) {
                Ok(json) => {
                    // Queue on the pipeline; actual send happens via `all()` below.
                    let _ = pipeline.publish::<(), _, _>(WS_MAIN_CHANNEL, json).await;
                }
                Err(e) => {
                    serialisation_errors += 1;
                    tracing::warn!(error = %e, "Failed to serialise WS envelope for pipeline");
                }
            }
        }

        if serialisation_errors > 0 {
            tracing::warn!(
                serialisation_errors,
                total = user_ids.len(),
                "Some envelopes were not included in the pipeline due to serialisation errors"
            );
        }

        if let Err(e) = pipeline.all::<Vec<i64>>().await {
            tracing::error!(error = %e, "WS pub/sub pipeline flush failed");
        } else {
            tracing::debug!(count = user_ids.len(), "Pipeline publish complete");
        }
    }

    /// Publish a payload to **every connected user on every instance**.
    /// Use for server-wide signals such as `HomeInvalidated`.
    #[tracing::instrument(
        name  = "pubsub.broadcast_all",
        skip  (self, payload),
        fields(event = %payload.event)
    )]
    pub async fn broadcast_all(&self, payload: WsPayload) {
        let envelope = WsPubSubEnvelope::broadcast(payload);
        self.publish(WS_MAIN_CHANNEL, envelope).await;
    }

    // ── Room membership ───────────────────────────────────────────────────────

    /// Register a user as a member of a broadcast room.
    ///
    /// Called when a user joins a live broadcast (HTTP join endpoint) **and**
    /// when they reconnect via WebSocket while the broadcast is still live.
    ///
    /// Internally this:
    /// 1. Adds the user to a Redis SET (`room:{broadcast_id}:members`).
    /// 2. Subscribes this instance to the room's Redis channel so it receives
    ///    messages published by any instance.
    #[tracing::instrument(
        name  = "pubsub.join_room",
        skip  (self),
        fields(user_id = %user_id, broadcast_id = %broadcast_id)
    )]
    pub async fn join_room(&self, user_id: Uuid, broadcast_id: Uuid) -> Result<()> {
        let key = room_member_key(broadcast_id);
        let user_str = user_id.to_string();

        // SADD + EXPIRE in two commands is fine here; the SET already existed
        // (created when the broadcast started), so there is no race.
        let _added: i64 = self.redis.sadd(&key, &user_str).await?;
        self.redis.expire(&key, ROOM_MEMBERSHIP_TTL_SECS).await?;

        // Subscribe this instance to the room channel.
        // If already subscribed (e.g., another user is in the same room on
        // this instance), fred deduplicates the SUBSCRIBE command.
        let channel = room_channel(broadcast_id);
        if let Err(e) = self.subscriber.subscribe(&channel).await {
            // Non-fatal: local delivery still works; log and continue.
            tracing::warn!(
                error       = %e,
                broadcast_id = %broadcast_id,
                "Failed to subscribe local instance to room channel"
            );
        }

        tracing::debug!(user_id = %user_id, broadcast_id = %broadcast_id, "User joined room");
        Ok(())
    }

    /// Remove a user from a broadcast room.
    ///
    /// Called when a user leaves (HTTP leave endpoint) or their WebSocket
    /// connection drops.
    ///
    /// If the room becomes empty,this instance unsubscribes from the channel
    /// and the Redis SET is deleted, reclaiming memory.
    #[tracing::instrument(
        name  = "pubsub.leave_room",
        skip  (self),
        fields(user_id = %user_id, broadcast_id = %broadcast_id)
    )]
    pub async fn leave_room(&self, user_id: Uuid, broadcast_id: Uuid) -> Result<()> {
        let key = room_member_key(broadcast_id);
        let user_str = user_id.to_string();

        let _removed: i64 = self.redis.srem(&key, &user_str).await?;

        let remaining: i64 = self.redis.scard(&key).await?;
        if remaining == 0 {
            // No one left in this room on any instance — clean up.
            let _ = self.redis.del(&key).await;
            let channel = room_channel(broadcast_id);
            if let Err(e) = self.subscriber.unsubscribe(&channel).await {
                tracing::warn!(
                    error        = %e,
                    broadcast_id = %broadcast_id,
                    "Failed to unsubscribe from empty room channel"
                );
            }
        }

        tracing::debug!(user_id = %user_id, broadcast_id = %broadcast_id, "User left room");
        Ok(())
    }

    /// Seed the room membership set when a broadcast goes live.
    ///
    /// At broadcast start the only participant is the host (creator).  Call
    /// this once so that subsequent joins have a valid SET to SADD into.
    #[tracing::instrument(
        name  = "pubsub.init_room",
        skip  (self, initial_participant_ids),
        fields(broadcast_id = %broadcast_id, count = initial_participant_ids.len())
    )]
    pub async fn init_room(
        &self,
        broadcast_id: Uuid,
        initial_participant_ids: &[Uuid],
    ) -> Result<()> {
        if initial_participant_ids.is_empty() {
            return Ok(());
        }

        let key = room_member_key(broadcast_id);
        let ids: Vec<String> = initial_participant_ids
            .iter()
            .map(|id| id.to_string())
            .collect();

        let _: i64 = self.redis.sadd(&key, ids).await?;
        self.redis.expire(&key, ROOM_MEMBERSHIP_TTL_SECS).await?;

        // Subscribe this instance to the room channel.
        let channel = room_channel(broadcast_id);
        if let Err(e) = self.subscriber.subscribe(&channel).await {
            tracing::warn!(
                error        = %e,
                broadcast_id = %broadcast_id,
                "Failed to subscribe to newly initialised room channel"
            );
        }

        tracing::info!(
            broadcast_id = %broadcast_id,
            initial      = initial_participant_ids.len(),
            "Room initialised"
        );
        Ok(())
    }

    /// Tear down a room when a broadcast ends.
    ///
    /// Deletes the Redis membership SET and unsubscribes every local listener
    /// from the channel.  This is safe to call even if the SET no longer exists.
    #[tracing::instrument(
        name  = "pubsub.destroy_room",
        skip  (self),
        fields(broadcast_id = %broadcast_id)
    )]
    pub async fn destroy_room(&self, broadcast_id: Uuid) -> Result<()> {
        let key = room_member_key(broadcast_id);
        let _ = self.redis.del(&key).await;

        let channel = room_channel(broadcast_id);
        if let Err(e) = self.subscriber.unsubscribe(&channel).await {
            // May already be unsubscribed if the room was empty — not an error.
            tracing::debug!(
                error        = %e,
                broadcast_id = %broadcast_id,
                "Unsubscribe from destroyed room (may already have been clean)"
            );
        }

        tracing::info!(broadcast_id = %broadcast_id, "Room destroyed");
        Ok(())
    }

    /// Check whether a user is currently a member of a room (cross-instance).
    pub async fn is_room_member(&self, broadcast_id: Uuid, user_id: Uuid) -> Result<bool> {
        let key = room_member_key(broadcast_id);
        let user_str = user_id.to_string();
        let result: bool = self.redis.sismember(&key, &user_str).await?;
        Ok(result)
    }

    /// Return the current member count for a room (cross-instance).
    pub async fn room_member_count(&self, broadcast_id: Uuid) -> Result<i64> {
        let key = room_member_key(broadcast_id);
        let count = self.redis.scard(&key).await?;
        Ok(count)
    }

    // ── Subscriber loop ───────────────────────────────────────────────────────

    /// Subscribe to the main channel and all room channels, then spawn the
    /// receive loop.
    ///
    /// **Call this exactly once**, immediately after building the router, before
    /// the server starts accepting connections.
    pub fn spawn_subscriber_loop(&self) {
        let subscriber = Arc::clone(&self.subscriber);
        let hub = self.hub.clone();

        // Subscribe to the main channel (user-targeted and broadcast messages).
        {
            let sub = Arc::clone(&subscriber);
            tokio::spawn(async move {
                match sub.subscribe(WS_MAIN_CHANNEL).await {
                    Ok(_) => {
                        tracing::info!(channel = WS_MAIN_CHANNEL, "Subscribed to main WS channel")
                    }
                    Err(e) => tracing::error!(error = %e, "Failed to subscribe to main WS channel"),
                }
            });
        }

        // PSubscribe to all room channels at once using a glob pattern.
        // New room channels (created when a broadcast starts) are automatically
        // matched without re-subscribing.
        {
            let sub = Arc::clone(&subscriber);
            tokio::spawn(async move {
                match sub.psubscribe(WS_ROOM_PATTERN).await {
                    Ok(_) => {
                        tracing::info!(pattern = WS_ROOM_PATTERN, "PSubscribed to room channels")
                    }
                    Err(e) => tracing::error!(error = %e, "Failed to PSubscribe to room channels"),
                }
            });
        }

        // Main receive loop — runs for the lifetime of the process.
        tokio::spawn(async move {
            run_subscriber_loop(subscriber, hub).await;
        });
    }

    /// Serialize an envelope and PUBLISH it to `channel` with exponential-backoff
    /// retry on transient Redis errors.
    async fn publish(&self, channel: &str, envelope: WsPubSubEnvelope) {
        let json = match serde_json::to_string(&envelope) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!(error = %e, channel = %channel, "Failed to serialise WS envelope");
                return;
            }
        };

        let mut attempts = 0u32;
        loop {
            attempts += 1;
            match self.publisher.publish::<i64, _, _>(channel, &json).await {
                Ok(_) => {
                    if attempts > 1 {
                        tracing::debug!(channel = %channel, attempts, "PUBLISH succeeded after retry");
                    }
                    return;
                }
                Err(e) if attempts >= MAX_PUBLISH_RETRIES => {
                    tracing::error!(
                        error   = %e,
                        channel = %channel,
                        attempts,
                        "PUBLISH failed after all retries — message dropped"
                    );
                    return;
                }
                Err(e) => {
                    let backoff = std::time::Duration::from_millis(100 * attempts as u64);
                    tracing::warn!(
                        error          = %e,
                        channel        = %channel,
                        attempt        = attempts,
                        next_retry_ms  = backoff.as_millis(),
                        "PUBLISH failed, retrying"
                    );
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }
}

/// Runs inside a dedicated Tokio task for the lifetime of the process.
///
/// Key properties:
/// - `message_rx()` returns a `BroadcastReceiver<Message>`.  If the receiver
///   falls behind the sender ring-buffer, `RecvError::Lagged` is returned.
///   This is acceptable: WS events are ephemeral and clients reconcile state
///   on reconnect via the offline-message buffer.
/// - `SubscriberClient` handles Redis reconnection automatically; we never
///   need to re-subscribe manually after a disconnect.
async fn run_subscriber_loop(subscriber: Arc<SubscriberClient>, hub: WsService) {
    let mut rx = subscriber.message_rx();

    loop {
        match rx.recv().await {
            Ok(msg) => {
                let json = match msg.value.as_str() {
                    Some(s) => s.to_owned(),
                    None => {
                        tracing::warn!(channel = %msg.channel, "Non-string pub/sub message — skipping");
                        continue;
                    }
                };

                let envelope = match serde_json::from_str::<WsPubSubEnvelope>(&json) {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!(error = %e, raw = %json, "Failed to deserialise envelope — skipping");
                        continue;
                    }
                };

                deliver_locally(&hub, envelope).await;
            }

            Err(RecvError::Lagged(n)) => {
                // This instance fell behind the pub/sub ring-buffer.  Some events
                // were dropped.  Clients will reconcile on their next reconnect.
                tracing::warn!(
                    skipped = n,
                    "WS pub/sub receiver lagged — {} events dropped; clients reconcile on reconnect",
                    n
                );
            }

            Err(RecvError::Closed) => {
                // The SubscriberClient was dropped or the process is shutting down.
                tracing::info!("WS pub/sub channel closed — subscriber loop exiting");
                break;
            }
        }
    }

    tracing::warn!("WS pub/sub subscriber loop has exited");
}

/// Deliver a deserialized envelope to local clients only.
///
/// Each API instance runs this independently after receiving the same
/// Redis message.  `WsService::send_to_room` / `send_to_user` /
/// `broadcast_all` only touch the local in-process `DashMap`, so there
/// is no cross-instance coordination here.
async fn deliver_locally(hub: &WsService, envelope: WsPubSubEnvelope) {
    match envelope {
        WsPubSubEnvelope::Room(e) => {
            hub.send_to_room(e.room_id, e.payload).await;
        }
        WsPubSubEnvelope::User(e) => {
            hub.send_to_user(e.user_id, e.payload).await;
        }
        WsPubSubEnvelope::Broadcast(e) => {
            hub.broadcast_all(e.payload).await;
        }
    }
}

/// Redis SET key that tracks which user IDs are members of a room.
fn room_member_key(broadcast_id: Uuid) -> RedisKey {
    RedisKey::new(format!("room:{}:members", broadcast_id))
}

/// Redis pub/sub channel name for a specific broadcast room.
fn room_channel(broadcast_id: Uuid) -> String {
    format!("{}{}", WS_ROOM_CHANNEL_PREFIX, broadcast_id)
}
