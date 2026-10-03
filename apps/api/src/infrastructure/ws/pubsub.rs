//! Cross-instance fan-out for WebSocket events, over Redis pub/sub.
//!
//! Ported from `apps/api/src/shared/services/ws/pubsub.rs` on `master` (`903c3ba`).
//!
//! # Why this exists
//!
//! On Render, each replica holds only the connections that landed on it (plan §3.1).
//! A chat message published by the replica serving the sender would reach that
//! sender's listeners and nobody else. Redis pub/sub is what makes "publish once,
//! deliver to every replica's local sockets" work without sticky sessions.
//!
//! # What changed
//!
//! - **Room membership goes through [`RedisKey`].** `master` built the membership key
//!   with a hand-rolled `format!("room:{id}:members")` and a bare
//!   `expire` afterwards. That bypassed the §3.2/§9.4 rule the key type exists to
//!   enforce — the TTL was optional and easy to omit. [`RedisKey::room_members`]
//!   carries it.
//! - **The subscribe decision is local, the membership set is global.** Kept from
//!   `master`, and worth restating: `hub.add_local_room_member` answers "does *this*
//!   instance need the channel?", while the Redis SET answers "is this user in the
//!   room, deployment-wide?". Conflating the two is how rooms end up with a
//!   subscription nobody reads.
//! - **`build` takes a URL, not `crate::config::Config`.** The `config` module is
//!   Step 3.7; taking the one string it would have supplied keeps this adapter
//!   independent of an application-state type it does not otherwise need.
//! - **Dead code removed.** `master` had a commented-out `subscribe` block in
//!   `init_room` and a stale "Targeted messages" doc comment on the channel constant.

use crate::infrastructure::redis::keys::{RedisKey, ttl};
use crate::infrastructure::redis::{Redis, RedisConfig};
use crate::infrastructure::ws::{WsPayload, WsService};
use anyhow::Result;
use fred::clients::SubscriberClient;
use fred::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

/// Channel carrying user-targeted and server-wide messages.
///
/// One channel for both because they share a delivery path: the receiving replica
/// looks at the envelope tag to decide between one recipient and everyone.
const WS_MAIN_CHANNEL: &str = "meno:ws:events";

/// Prefix of the per-broadcast room channels: `meno:ws:room:{broadcast_id}`.
const WS_ROOM_CHANNEL_PREFIX: &str = "meno:ws:room:";

/// Glob matching every room channel at once.
const WS_ROOM_PATTERN: &str = "meno:ws:room:*";

/// PUBLISH attempts before a message is dropped.
///
/// WS events are ephemeral and the client reconciles on reconnect, so a bounded
/// retry beats an unbounded one that would hold a request open against a Redis that
/// is not coming back.
const MAX_PUBLISH_RETRIES: u32 = 3;

/// First retry backoff, in milliseconds. Doubles per attempt.
const PUBLISH_BACKOFF_BASE_MS: u64 = 100;

/// User fan-out at or below this size goes out as individual PUBLISHes.
///
/// Below the threshold the pipeline's own overhead costs more round trips than it
/// saves; `master` used the same 10.
const PIPELINE_PUBLISH_THRESHOLD: usize = 10;

/// Delivery to every local listener in one broadcast room.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsRoomEnvelope {
    /// The room.
    pub room_id: Uuid,
    /// What to deliver.
    pub payload: WsPayload,
}

/// Delivery to one user, on whichever replica holds their socket.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsUserEnvelope {
    /// The recipient.
    pub user_id: Uuid,
    /// What to deliver.
    pub payload: WsPayload,
}

/// Delivery to every connected user on every replica.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsBroadcastEnvelope {
    /// What to deliver.
    pub payload: WsPayload,
}

/// A tagged envelope: the `type` tells a receiving replica which delivery path to take.
///
/// One enum rather than three channels, because a replica already subscribes to the
/// room pattern and the main channel; adding a channel per audience would mean three
/// subscribe calls and three reconnect paths.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum WsPubSubEnvelope {
    /// Room-scoped.
    Room(WsRoomEnvelope),
    /// Single-recipient.
    User(WsUserEnvelope),
    /// Everyone.
    Broadcast(WsBroadcastEnvelope),
}

impl WsPubSubEnvelope {
    /// Wrap a room-scoped delivery.
    #[must_use]
    pub fn room(room_id: Uuid, payload: WsPayload) -> Self {
        Self::Room(WsRoomEnvelope { room_id, payload })
    }

    /// Wrap a single-recipient delivery.
    #[must_use]
    pub fn user(user_id: Uuid, payload: WsPayload) -> Self {
        Self::User(WsUserEnvelope { user_id, payload })
    }

    /// Wrap a server-wide delivery.
    #[must_use]
    pub fn broadcast(payload: WsPayload) -> Self {
        Self::Broadcast(WsBroadcastEnvelope { payload })
    }

    /// The payload, whichever variant this is.
    ///
    /// Used by the subscriber loop for logging and by the tests, so a new variant does
    /// not need the accessor touched.
    #[must_use]
    pub fn payload(&self) -> &WsPayload {
        match self {
            Self::Room(e) => &e.payload,
            Self::User(e) => &e.payload,
            Self::Broadcast(e) => &e.payload,
        }
    }
}

/// The bridge between application logic and Redis pub/sub.
///
/// ## Connection model
///
/// Two Redis connections, because Redis refuses to mix the two roles:
///
/// - `publisher` — a normal client, used only for `PUBLISH`.
/// - `subscriber` — a dedicated [`SubscriberClient`] holding the persistent
///   `SUBSCRIBE`/`PSUBSCRIBE`.
///
/// ## Scaling
///
/// Every replica independently:
///
/// 1. Subscribes to [`WS_MAIN_CHANNEL`].
/// 2. `PSUBSCRIBE`s to [`WS_ROOM_PATTERN`], so rooms created after boot are matched
///    without re-subscribing.
/// 3. Delivers each received message only to sockets local to it — the pub/sub hop
///    *is* the cross-instance routing, so nothing coordinates per message.
#[derive(Clone)]
pub struct WsPubSubBridge {
    /// Pooled client, `PUBLISH` only.
    publisher: Client,
    /// Dedicated subscriber, `SUBSCRIBE`/`PSUBSCRIBE` only.
    subscriber: Arc<SubscriberClient>,
    /// The local socket registry this bridge delivers into.
    hub: WsService,
    /// Shared client for the room-membership SET.
    redis: Redis,
}

impl WsPubSubBridge {
    /// Build the publisher and subscriber from a Redis URL and verify both connect.
    ///
    /// Takes the URL rather than the application `Config` so this adapter does not
    /// depend on a state type it has no other use for.
    ///
    /// # Errors
    ///
    /// Fails if either client cannot be built or reach Redis. Subscribing is deferred
    /// to [`Self::spawn_subscriber_loop`], so a bridge can be constructed before the
    /// server starts accepting sockets.
    pub async fn build(redis_url: &str, hub: WsService, redis: Redis) -> Result<Self> {
        let builder = Builder::from_config(Config::from_url(redis_url)?);

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

    /// Build the bridge from the same config the shared [`Redis`] uses.
    ///
    /// The convenience path for wiring: the membership SET and the pub/sub hop talk
    /// to the same instance, and taking one config makes it impossible to point them
    /// at different Redis servers.
    ///
    /// # Errors
    ///
    /// Propagates any failure from [`Self::build`].
    pub async fn build_from_config(
        config: &RedisConfig,
        hub: WsService,
        redis: Redis,
    ) -> Result<Self> {
        Self::build(&config.url, hub, redis).await
    }

    /// Publish to every participant in a live broadcast room.
    ///
    /// The primary fan-out path: one `PUBLISH` regardless of how many participants
    /// there are, and no database queries.
    ///
    /// Errors are logged, not returned. A caller in the middle of a chat send should
    /// not fail because a live-realtime hint could not be delivered — the message is
    /// already persisted.
    #[tracing::instrument(
        name  = "pubsub.publish_to_room",
        skip  (self, payload),
        fields(broadcast_id = %broadcast_id, event = %payload.event)
    )]
    pub async fn publish_to_room(&self, broadcast_id: Uuid, payload: WsPayload) {
        self.publish(
            &room_channel(broadcast_id),
            WsPubSubEnvelope::room(broadcast_id, payload),
        )
        .await;
    }

    /// Publish to one user, on whichever replica holds their socket.
    #[tracing::instrument(
        name  = "pubsub.publish_to_user",
        skip  (self, payload),
        fields(user_id = %user_id, event = %payload.event)
    )]
    pub async fn publish_to_user(&self, user_id: Uuid, payload: WsPayload) {
        self.publish(WS_MAIN_CHANNEL, WsPubSubEnvelope::user(user_id, payload))
            .await;
    }

    /// Publish to many users.
    ///
    /// Small batches go out as individual `PUBLISH`es; larger ones are pipelined into
    /// a single round trip.
    #[tracing::instrument(
        name  = "pubsub.publish_to_users",
        skip  (self, payload),
        fields(count = user_ids.len(), event = %payload.event)
    )]
    pub async fn publish_to_users(&self, user_ids: &[Uuid], payload: WsPayload) {
        match user_ids.len() {
            0 => return,
            n if n <= PIPELINE_PUBLISH_THRESHOLD => {
                for &uid in user_ids {
                    self.publish_to_user(uid, payload.clone()).await;
                }
            }
            _ => self.publish_pipelined(user_ids, payload).await,
        }
    }

    /// One pipelined round trip for every recipient.
    async fn publish_pipelined(&self, user_ids: &[Uuid], payload: WsPayload) {
        let pipeline = self.publisher.pipeline();
        let mut queued = 0usize;

        for &uid in user_ids {
            let envelope = WsPubSubEnvelope::user(uid, payload.clone());
            match serde_json::to_string(&envelope) {
                Ok(json) => {
                    if let Err(e) = pipeline.publish::<(), _, _>(WS_MAIN_CHANNEL, json).await {
                        tracing::warn!(error = %e, user_id = %uid, "failed to queue WS publish");
                    }
                    queued += 1;
                }
                Err(e) => {
                    tracing::warn!(error = %e, user_id = %uid, "failed to serialise WS envelope")
                }
            }
        }

        if let Err(e) = pipeline.all::<Vec<i64>>().await {
            tracing::error!(error = %e, queued, "WS pub/sub pipeline flush failed");
        } else {
            tracing::debug!(queued, "pipeline publish complete");
        }
    }

    /// Publish to every connected user on every replica.
    ///
    /// For server-wide signals such as `homeInvalidated`.
    #[tracing::instrument(
        name  = "pubsub.broadcast_all",
        skip  (self, payload),
        fields(event = %payload.event)
    )]
    pub async fn broadcast_all(&self, payload: WsPayload) {
        self.publish(WS_MAIN_CHANNEL, WsPubSubEnvelope::broadcast(payload))
            .await;
    }

    // ── room membership ─────────────────────────────────────────────────────

    /// Register a user as a member of a broadcast room.
    ///
    /// Called when a user joins over HTTP *and* when they reconnect over the socket
    /// while the broadcast is still live. Adds them to the shared SET, then subscribes
    /// this replica to the room's channel if it had no local listener yet.
    ///
    /// # Errors
    ///
    /// Fails if the membership `SADD` fails; the subscribe failure is logged and
    /// swallowed, because losing the subscription degrades cross-instance delivery but
    /// local delivery still works.
    #[tracing::instrument(
        name  = "pubsub.join_room",
        skip  (self),
        fields(user_id = %user_id, broadcast_id = %broadcast_id)
    )]
    pub async fn join_room(&self, user_id: Uuid, broadcast_id: Uuid) -> Result<()> {
        // `sadd` applies the key's TTL on creation, so the SET cannot outlive its
        // broadcast (§9.4). Refreshing it on every join is deliberate: an active room
        // must not expire out from under long-lived participants.
        let key = RedisKey::room_members(broadcast_id, ttl::ROOM_MEMBERS);
        let _: i64 = self.redis.sadd(&key, user_id.to_string()).await?;
        self.redis.expire(&key, ttl::ROOM_MEMBERS).await?;

        if self
            .hub
            .registry()
            .add_local_room_member(broadcast_id, user_id)
            && let Err(e) = self.subscriber.subscribe(room_channel(broadcast_id)).await
        {
            tracing::warn!(
                error = %e,
                broadcast_id = %broadcast_id,
                "failed to subscribe to room channel"
            );
        }

        tracing::debug!(user_id = %user_id, broadcast_id = %broadcast_id, "user joined room");
        Ok(())
    }

    /// Remove a user from a broadcast room.
    ///
    /// The SET is deleted once it empties rather than waiting for its TTL, and this
    /// replica unsubscribes when it holds no local member — otherwise a replica that
    /// once served a room keeps a subscription and a key for a broadcast that ended
    /// hours ago.
    ///
    /// # Errors
    ///
    /// Fails if the membership `SREM` fails. An unsubscribe failure is logged only.
    #[tracing::instrument(
        name  = "pubsub.leave_room",
        skip  (self),
        fields(user_id = %user_id, broadcast_id = %broadcast_id)
    )]
    pub async fn leave_room(&self, user_id: Uuid, broadcast_id: Uuid) -> Result<()> {
        let key = RedisKey::room_members(broadcast_id, ttl::ROOM_MEMBERS);
        let _: i64 = self.redis.srem(&key, user_id.to_string()).await?;

        // `SCARD` returns 0 for a key that does not exist, so this also covers the
        // case where the SET was already reclaimed.
        if self.redis.scard::<i64>(&key).await? == 0 {
            let _ = self.redis.del(&key).await;
        }

        if self
            .hub
            .registry()
            .remove_local_room_member(broadcast_id, user_id)
            && let Err(e) = self
                .subscriber
                .unsubscribe(room_channel(broadcast_id))
                .await
        {
            tracing::warn!(
                error = %e,
                broadcast_id = %broadcast_id,
                "failed to unsubscribe from room channel"
            );
        }

        tracing::debug!(user_id = %user_id, broadcast_id = %broadcast_id, "user left room");
        Ok(())
    }

    /// Seed a room's membership SET when a broadcast goes live.
    ///
    /// At broadcast start the host is the only member; subsequent joins `SADD` into
    /// this SET.
    ///
    /// No subscription is made here — `master` had that block commented out, and
    /// correctly so: the host's socket has not been registered at the moment a
    /// broadcast goes live, so there is nothing local to deliver to yet. The
    /// subscription happens in `join_room`, when there is.
    ///
    /// # Errors
    ///
    /// Fails if the membership `SADD` fails.
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

        let key = RedisKey::room_members(broadcast_id, ttl::ROOM_MEMBERS);
        let ids: Vec<String> = initial_participant_ids
            .iter()
            .map(Uuid::to_string)
            .collect();

        let _: i64 = self.redis.sadd(&key, ids).await?;
        self.redis.expire(&key, ttl::ROOM_MEMBERS).await?;

        tracing::info!(
            broadcast_id = %broadcast_id,
            initial = initial_participant_ids.len(),
            "room initialised"
        );
        Ok(())
    }

    /// Tear down a room's shared state once the broadcast ends.
    ///
    /// Deletes the membership SET. Every replica runs this independently when it sees
    /// the end event; deleting an already-deleted key is not an error.
    ///
    /// # Errors
    ///
    /// Currently infallible, but returns `Result` to match the other room
    /// operations and to leave room for the delete to start reporting.
    #[tracing::instrument(name = "pubsub.destroy_room", skip(self), fields(broadcast_id = %broadcast_id))]
    pub async fn destroy_room(&self, broadcast_id: Uuid) -> Result<()> {
        let key = RedisKey::room_members(broadcast_id, ttl::ROOM_MEMBERS);
        let _ = self.redis.del(&key).await;
        tracing::info!(broadcast_id = %broadcast_id, "room destroyed");
        Ok(())
    }

    /// Drop this replica's local membership of a room and unsubscribe.
    ///
    /// Called when a replica observes the end event through the room channel. There
    /// is no single owner that could unsubscribe on everyone else's behalf, so each
    /// replica does it for itself.
    pub async fn teardown_local_room(&self, broadcast_id: Uuid) {
        if !self
            .hub
            .registry()
            .clear_local_room(broadcast_id)
            .is_empty()
            && let Err(e) = self
                .subscriber
                .unsubscribe(room_channel(broadcast_id))
                .await
        {
            tracing::warn!(
                error = %e,
                broadcast_id = %broadcast_id,
                "failed to unsubscribe after room teardown"
            );
        }
    }

    /// Whether a user is in a room, deployment-wide.
    ///
    /// # Errors
    ///
    /// Fails if the Redis `SISMEMBER` fails.
    pub async fn is_room_member(&self, broadcast_id: Uuid, user_id: Uuid) -> Result<bool> {
        let key = RedisKey::room_members(broadcast_id, ttl::ROOM_MEMBERS);
        Ok(self
            .redis
            .sismember::<bool, _>(&key, user_id.to_string())
            .await?)
    }

    /// How many users are in a room, deployment-wide.
    ///
    /// # Errors
    ///
    /// Fails if the Redis `SCARD` fails.
    pub async fn room_member_count(&self, broadcast_id: Uuid) -> Result<i64> {
        let key = RedisKey::room_members(broadcast_id, ttl::ROOM_MEMBERS);
        Ok(self.redis.scard(&key).await?)
    }

    // ── subscriber loop ─────────────────────────────────────────────────────

    /// Subscribe to every channel and spawn the receive loop.
    ///
    /// **Call exactly once**, after building the bridge and before the server accepts
    /// connections. Returns immediately; the loop runs for the process's lifetime.
    pub fn spawn_subscriber_loop(&self) {
        let subscriber = Arc::clone(&self.subscriber);

        // The main channel. Retried by fred, so a failure here is logged and the
        // loop below still starts — it will pick messages up once fred reconnects.
        {
            let sub = Arc::clone(&subscriber);
            tokio::spawn(async move {
                match sub.subscribe(WS_MAIN_CHANNEL).await {
                    Ok(_) => {
                        tracing::info!(channel = WS_MAIN_CHANNEL, "subscribed to main WS channel")
                    }
                    Err(e) => tracing::error!(error = %e, "failed to subscribe to main WS channel"),
                }
            });
        }

        // Every room channel at once, so a broadcast that goes live after boot is
        // matched without re-subscribing.
        {
            let sub = Arc::clone(&subscriber);
            tokio::spawn(async move {
                match sub.psubscribe(WS_ROOM_PATTERN).await {
                    Ok(_) => {
                        tracing::info!(pattern = WS_ROOM_PATTERN, "PSubscribed to room channels")
                    }
                    Err(e) => tracing::error!(error = %e, "failed to PSubscribe to room channels"),
                }
            });
        }

        let bridge = self.clone();
        tokio::spawn(async move {
            run_subscriber_loop(subscriber, bridge).await;
        });
    }

    /// Serialise and publish, with bounded exponential backoff.
    ///
    /// A successful publish that took more than one attempt is worth a debug line:
    /// sustained retries mean the pool is undersized for §3.2's budget.
    async fn publish(&self, channel: &str, envelope: WsPubSubEnvelope) {
        let json = match serde_json::to_string(&envelope) {
            Ok(json) => json,
            Err(e) => {
                tracing::error!(error = %e, channel = %channel, "failed to serialise WS envelope");
                return;
            }
        };

        for attempt in 1..=MAX_PUBLISH_RETRIES {
            match self.publisher.publish::<i64, _, _>(channel, &json).await {
                Ok(_) => {
                    if attempt > 1 {
                        tracing::debug!(channel = %channel, attempt, "PUBLISH succeeded after retry");
                    }
                    return;
                }
                Err(e) if attempt == MAX_PUBLISH_RETRIES => {
                    tracing::error!(
                        error = %e,
                        channel = %channel,
                        attempt,
                        "PUBLISH failed after all retries — message dropped"
                    );
                    return;
                }
                Err(e) => {
                    let backoff = std::time::Duration::from_millis(
                        PUBLISH_BACKOFF_BASE_MS * u64::from(attempt),
                    );
                    tracing::warn!(
                        error = %e,
                        channel = %channel,
                        attempt,
                        next_retry_ms = backoff.as_millis(),
                        "PUBLISH failed, retrying"
                    );
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }
}

/// The receive loop, running for the process's lifetime.
///
/// Two properties worth knowing:
///
/// - `RecvError::Lagged` means this replica fell behind the subscriber ring buffer
///   and some events were dropped. That is acceptable: WS events are hints, and the
///   client reconciles from the offline buffer and a refetch on reconnect.
/// - Reconnection is fred's job. Nothing here re-subscribes, because a
///   `SubscriberClient` restores its own subscriptions after a reconnect.
async fn run_subscriber_loop(subscriber: Arc<SubscriberClient>, bridge: WsPubSubBridge) {
    let mut rx = subscriber.message_rx();

    loop {
        match rx.recv().await {
            Ok(msg) => {
                let Some(json) = msg.value.as_str() else {
                    tracing::warn!(channel = %msg.channel, "non-string pub/sub message — skipping");
                    continue;
                };

                match serde_json::from_str::<WsPubSubEnvelope>(&json) {
                    Ok(envelope) => deliver_locally(&bridge, envelope).await,
                    Err(e) => {
                        // Logged with the raw body: an envelope that will not parse is
                        // usually a version skew between replicas, and the payload is
                        // the only evidence of which side is wrong.
                        tracing::warn!(error = %e, raw = %json, "failed to deserialise envelope — skipping");
                    }
                }
            }

            Err(RecvError::Lagged(n)) => {
                tracing::warn!(
                    skipped = n,
                    "WS pub/sub receiver lagged — events dropped; clients reconcile on reconnect"
                );
            }

            Err(RecvError::Closed) => {
                tracing::info!("WS pub/sub channel closed — subscriber loop exiting");
                break;
            }
        }
    }

    tracing::warn!("WS pub/sub subscriber loop has exited");
}

/// Deliver one envelope to local sockets.
///
/// Every replica runs this independently on the same message. The `WsService` methods
/// it calls touch only the local `DashMap`s, so no per-message coordination happens
/// across replicas — the pub/sub hop is the routing.
async fn deliver_locally(bridge: &WsPubSubBridge, envelope: WsPubSubEnvelope) {
    match envelope {
        WsPubSubEnvelope::Room(e) => {
            let terminates = e.payload.event.terminates_room();
            let delivered = bridge.hub.send_to_room(e.room_id, e.payload).await;
            if terminates {
                bridge.teardown_local_room(e.room_id).await;
            }
            tracing::debug!(room_id = %e.room_id, delivered, "room envelope delivered locally");
        }
        WsPubSubEnvelope::User(e) => bridge.hub.send_to_user(e.user_id, e.payload).await,
        WsPubSubEnvelope::Broadcast(e) => {
            let delivered = bridge.hub.broadcast_all(e.payload).await;
            tracing::debug!(delivered, "broadcast envelope delivered locally");
        }
    }
}

/// Redis pub/sub channel for one broadcast room.
fn room_channel(broadcast_id: Uuid) -> String {
    format!("{WS_ROOM_CHANNEL_PREFIX}{broadcast_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::ws::model::WsEvent;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn payload() -> WsPayload {
        WsPayload::host_reconnected(id(1))
    }

    #[test]
    fn envelopes_are_tagged_so_the_receiver_can_pick_a_path() {
        // The tag is the whole mechanism: a replica subscribed to both the main
        // channel and the room pattern has no other way to know which path to take.
        let json = serde_json::to_value(WsPubSubEnvelope::room(id(1), payload())).expect("room");
        assert_eq!(json["type"], "room");
        assert_eq!(json["data"]["roomId"], id(1).to_string());

        let json = serde_json::to_value(WsPubSubEnvelope::user(id(2), payload())).expect("user");
        assert_eq!(json["type"], "user");
        assert_eq!(json["data"]["userId"], id(2).to_string());

        let json = serde_json::to_value(WsPubSubEnvelope::broadcast(payload())).expect("broadcast");
        assert_eq!(json["type"], "broadcast");
    }

    #[test]
    fn envelope_payload_accessor_survives_every_variant() {
        // A new variant that forgets the accessor would fail here rather than in the
        // subscriber loop.
        for envelope in [
            WsPubSubEnvelope::room(id(1), payload()),
            WsPubSubEnvelope::user(id(1), payload()),
            WsPubSubEnvelope::broadcast(payload()),
        ] {
            assert_eq!(envelope.payload().event, WsEvent::HostReconnected);
        }
    }

    #[test]
    fn envelopes_round_trip_through_the_wire() {
        // One replica serialises, another deserialises. Anything that does not survive
        // a round trip is dropped mid-fan-out, silently.
        for envelope in [
            WsPubSubEnvelope::room(id(1), payload()),
            WsPubSubEnvelope::user(id(2), payload()),
            WsPubSubEnvelope::broadcast(payload()),
            WsPubSubEnvelope::user(
                id(3),
                WsPayload::error(
                    id(3),
                    crate::infrastructure::ws::errors::WsErrorCode::KickedFromRoom,
                    "removed",
                ),
            ),
        ] {
            let json = serde_json::to_string(&envelope).expect("serialise");
            let back: WsPubSubEnvelope = serde_json::from_str(&json).expect("deserialise");
            assert_eq!(back, envelope, "round trip failed for {json}");
        }
    }

    #[test]
    fn an_unknown_envelope_tag_is_rejected_rather_than_guessed() {
        assert!(
            serde_json::from_str::<WsPubSubEnvelope>(r#"{"type":"nonsense","data":{}}"#).is_err()
        );
        // A malformed body must not be routed by falling through to a default.
        assert!(serde_json::from_str::<WsPubSubEnvelope>(r#"{"type":"user"}"#).is_err());
    }

    #[test]
    fn room_channels_match_the_subscribed_glob() {
        // A room channel that does not match the pattern is a room this replica never
        // hears about — cross-instance delivery silently does nothing.
        let channel = room_channel(id(1));
        assert!(channel.starts_with(WS_ROOM_CHANNEL_PREFIX));
        assert!(channel.ends_with(&id(1).to_string()));
        assert!(
            glob_matches(WS_ROOM_PATTERN, &channel),
            "{channel} does not match {WS_ROOM_PATTERN}"
        );
    }

    #[test]
    fn room_channels_are_distinct_per_broadcast() {
        // A collision would deliver one broadcast's chat into another's room.
        assert_ne!(room_channel(id(1)), room_channel(id(2)));
        assert!(glob_matches(WS_ROOM_PATTERN, &room_channel(id(9))));
    }

    #[test]
    fn the_main_channel_does_not_match_the_room_glob() {
        // If it did, every user-targeted message would also be delivered to whatever
        // room a replica happened to be subscribed to.
        assert!(!glob_matches(WS_ROOM_PATTERN, WS_MAIN_CHANNEL));
    }

    #[test]
    fn publish_retry_budget_is_bounded() {
        // An unbounded retry would hold a request open against a Redis that is not
        // coming back, turning a realtime-hint failure into a request failure.
        // These are compile-time constants, so the bound is checked at compile time.
        const {
            assert!(
                MAX_PUBLISH_RETRIES >= 1,
                "a zero budget drops every message"
            );
            assert!(
                MAX_PUBLISH_RETRIES <= 5,
                "more than a handful of retries is not a bound, it is a hang"
            );
            assert!(PUBLISH_BACKOFF_BASE_MS > 0);
        }
    }

    #[test]
    fn pipeline_threshold_keeps_small_batches_on_the_simple_path() {
        // A pipeline for one recipient costs more than the round trip it saves.
        const {
            assert!(PIPELINE_PUBLISH_THRESHOLD >= 5);
        }
    }

    /// Minimal glob matcher for a trailing `*`, which is all the channel patterns use.
    fn glob_matches(pattern: &str, value: &str) -> bool {
        match pattern.strip_suffix('*') {
            Some(prefix) => value.starts_with(prefix),
            None => pattern == value,
        }
    }
}
