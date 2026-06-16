use crate::config::MenoConfig;
use crate::shared::services::ws::WsService;
use crate::shared::services::ws::dto::WsPayload;
use fred::clients::SubscriberClient;
use fred::prelude::*;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

/// The single Redis channel name all instances subscribe to.
const WS_PUBSUB_CHANNEL: &str = "meno:ws:events";

/// Envelope wrapping every cross-instance message.
/// `target_user_id` is None for broadcast-all; Some(id) for targeted delivery.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct WsPubSubEnvelope {
    /// The user to deliver to. None = broadcast to all connected clients.
    pub target_user_id: Option<Uuid>,

    /// The actual payload the client receives.
    pub payload: WsPayload,
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
    /// Regular client — used only for PUBLISH.
    publisher: Client,

    /// Dedicated subscriber — used only for SUBSCRIBE / message receive.
    /// Wrapped in Arc so it can be moved into the background task while
    /// the bridge is also cloned into AppState.
    subscriber: Arc<SubscriberClient>,

    hub: WsService,
}
impl WsPubSubBridge {
    pub async fn build(config: &MenoConfig, hub: WsService) -> anyhow::Result<Self> {
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
        })
    }

    /// Publish a targeted WS message so every instance can try to deliver it.
    #[tracing::instrument(
        name = "pubsub.publish_to_user",
        skip(self, payload),
        fields(target_user_id = %user_id)
    )]
    pub async fn publish_to_user(&self, user_id: Uuid, payload: WsPayload) {
        let envelope = WsPubSubEnvelope {
            target_user_id: Some(user_id),
            payload,
        };
        self.publish(envelope).await;
    }

    /// Publish to every connected client across all instances.
    #[tracing::instrument(name = "pubsub.publish_all", skip(self, payload))]
    pub async fn publish_all(&self, payload: WsPayload) {
        let envelope = WsPubSubEnvelope {
            target_user_id: None,
            payload,
        };
        self.publish(envelope).await;
    }

    /// Fan-out: publish to a slice of users.
    ///
    /// One PUBLISH per user keeps delivery logic simple. For very large fan-outs
    /// (thousands of subscribers) you can switch to a Vec<Uuid> envelope and
    /// batch into a single PUBLISH — optimize when profiling shows it matters.
    pub async fn publish_to_users(&self, user_ids: &[Uuid], payload: WsPayload) {
        let arc_payload = Arc::new(payload);
        for &id in user_ids {
            self.publish_to_user(id, (*arc_payload).clone()).await;
        }
    }

    /// Spawn the subscriber receive loop as a background Tokio task.
    ///
    /// Call once from `build_meno_router` — after this, the bridge is ready.
    /// The task runs until the process exits; no join handle is needed.
    ///
    /// ```rust
    /// bridge.spawn_subscriber_loop();
    /// ```
    pub fn spawn_subscriber_loop(&self) {
        let subscriber = Arc::clone(&self.subscriber);
        let hub = self.hub.clone();

        tokio::spawn(async move {
            run_subscriber_loop(subscriber, hub).await;
        });
    }

    async fn publish(&self, envelope: WsPubSubEnvelope) {
        let json = match serde_json::to_string(&envelope) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!(error = %e, "Failed to serialise WS pub/sub envelope");
                return;
            }
        };

        if let Err(e) = self
            .publisher
            .publish::<i64, _, _>(WS_PUBSUB_CHANNEL, json)
            .await
        {
            // Non-fatal — log and move on. The client may miss one event but
            // will reconcile via the next poll / reconnect drain.
            tracing::warn!(
                error = %e,
                channel = WS_PUBSUB_CHANNEL,
                "WS pub/sub publish failed"
            );
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
    // Spawn fred's built-in reconnect-and-resubscribe task.
    // This must be called BEFORE `subscribe()` so fred knows which channels
    // to restore after a Redis restart.
    let _manage_task = subscriber.manage_subscriptions();

    // Subscribe to the single shared channel
    if let Err(e) = subscriber.subscribe(WS_PUBSUB_CHANNEL).await {
        tracing::error!(
            error = %e,
            channel = WS_PUBSUB_CHANNEL,
            "Failed to subscribe to pub/sub channel. Exiting loop"
        );
        return;
    }

    tracing::info!(
        channel = WS_PUBSUB_CHANNEL,
        "Redis pub/sub subscriber loop running"
    );

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
}

/// Deliver an envelope to whichever clients are on THIS instance.
/// The local WsHub handles buffering for offline users automatically.
async fn deliver_locally(hub: &WsService, envelope: WsPubSubEnvelope) {
    match envelope.target_user_id {
        Some(user_id) => {
            hub.send_to_user(user_id, envelope.payload).await;
        }
        None => {
            hub.broadcast_all(envelope.payload).await;
        }
    }
}
