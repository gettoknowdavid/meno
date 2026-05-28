use crate::modules::auth::model::User;
use crate::shared::constants::TTL_3600_SECS;
use crate::shared::services::redis::keys::RedisKey;
use crate::shared::services::ws::dto::{ClientMessage, WsErrorCode, WsPayload, WsQuery};
use crate::shared::services::ws::model::{HeartbeatConfig, WsEvent};
use crate::state::MenoState;
use axum::{
    Json,
    extract::ws::{Message, WebSocket},
    extract::{Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::Response,
};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, stream::StreamExt};
use serde_json::{Value, json};
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, atomic};
use tokio::{
    sync::{Mutex, mpsc},
    time,
};
use uuid::Uuid;

/// GET /ws?token=<access_jwt>
pub async fn ws_upgrade(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<Arc<MenoState>>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let claims = state
        .jwt
        .decode_access(&query.token)
        .map_err(|_| error_response(StatusCode::UNAUTHORIZED, "Invalid token"))?;

    if !claims.verified {
        return Err(error_response(StatusCode::FORBIDDEN, "EMAIL_NOT_VERIFIED"));
    }

    // Check reconnect rate limit before upgrading
    if let Err(e) = check_reconnect_rate(&state, claims.sub).await {
        return Err(e);
    }

    let user = state
        .auth
        .find_user_by_id(claims.sub)
        .await
        .map_err(|_| error_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Error"))?
        .ok_or(error_response(StatusCode::BAD_REQUEST, "User not found"))?;

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, user, state)))
}

/// Main WebSocket connection handler
///
/// Implements:
/// - Tiered heartbeat (hosts get longer timeout)
/// - Grace period tracking
/// - Message draining on reconnect
/// - Clean disconnect handling
async fn handle_socket(socket: WebSocket, user: User, state: Arc<MenoState>) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));

    let (hub_tx, hub_rx) = mpsc::channel::<Arc<WsPayload>>(128);
    let conn_id = state.ws.register(user.id, hub_tx);

    let welcome = WsPayload::new(
        WsEvent::Notification,
        json!({
            "title": "Connected",
            "body": "Welcome to Meno Live Broadcast",
            "type": "system"
        }),
    );
    let _ = state.ws.send_to_user(user.id, welcome).await;

    // Drain any buffered messages from previous disconnects
    let buffered_messages = state.ws.drain_message_buffer(user.id).await;
    if !buffered_messages.is_empty() {
        tracing::info!(
            "Replaying {} buffered messages to user {}",
            buffered_messages.len(),
            user.id
        );

        let sender_lock = ws_sender.clone();
        let mut sender = sender_lock.lock().await;

        for payload in buffered_messages {
            let json = serde_json::to_string(&payload).unwrap_or_default();
            let _ = sender.send(Message::Text(json.into())).await;
        }
    }

    // Check if user is an active host (for heartbeat tuning)
    let is_host = state
        .broadcast
        .is_active_host(user.id)
        .await
        .unwrap_or(false);

    let heartbeat_config = HeartbeatConfig::default();
    let pong_timeout = if is_host {
        time::Duration::from_secs(heartbeat_config.host_pong_timeout_secs)
    } else {
        time::Duration::from_secs(heartbeat_config.listener_pong_timeout_secs)
    };

    let missed_pongs = Arc::new(AtomicU32::new(0));

    // Set presence in Redis
    let presence_key = RedisKey::presence(user.id);
    let _ = state.redis.set_ex(&presence_key, "1", 120).await;

    // Heartbeat task (sends periodic pings)
    let heartbeat_task = start_heartbeat_task(
        ws_sender.clone(),
        missed_pongs.clone(),
        heartbeat_config.clone(),
        user.id,
    );

    // Spawn the write task
    let write_task = start_write_task(ws_sender.clone(), hub_rx);

    // Main read loop
    let is_disconnected = run_read_loop(
        &mut ws_receiver,
        &state,
        user.id,
        &missed_pongs,
        pong_timeout,
        heartbeat_config.max_missed_pings,
    )
    .await;

    // Cleanup
    heartbeat_task.abort();
    write_task.abort();
    state.ws.unregister(user.id, conn_id);

    // Remove presence
    let _ = state.redis.del(&presence_key).await;

    // Handle disconnection cleanup
    if is_disconnected {
        handle_disconnect(&state, user.id, is_host).await;
    }
}

/// Handle incoming client messages
/// Handle incoming client messages
async fn handle_client_message(state: &MenoState, user_id: Uuid, raw_text: &str) {
    let msg: ClientMessage = match serde_json::from_str(raw_text) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("Invalid JSON from user {}: {}", user_id, e);
            return;
        }
    };

    match msg.event {
        WsEvent::Heartbeat => {
            // === SEND ACKNOWLEDGMENT BACK TO CLIENT ===
            let ack_payload = WsPayload::new(
                WsEvent::Heartbeat,
                json!({
                    "status": "ok",
                    "timestamp": chrono::Utc::now().timestamp(),
                    "userId": user_id
                }),
            );

            let _ = state.ws.send_to_user(user_id, ack_payload).await;

            // Refresh presence in Redis
            let presence_key = RedisKey::presence(user_id);
            let _ = state.redis.expire(&presence_key, 120).await;

            tracing::debug!("Heartbeat received and acknowledged for user {}", user_id);
        }

        _ => {
            tracing::warn!("Client sent unsupported event: {:?}", msg.event);
            let _ = state
                .ws
                .send_error(
                    user_id,
                    Uuid::nil(),
                    WsErrorCode::Unsupported,
                    format!("Unsupported event: {}", msg.event),
                )
                .await;
        }
    }
}

/// Handle disconnection - may trigger host grace period
async fn handle_disconnect(state: &MenoState, user_id: Uuid, is_host: bool) {
    if !is_host {
        // Regular listener disconnect - handled by participant leave HTTP endpoint
        // The client should call DELETE /broadcasts/:id/participant on disconnect
        return;
    }

    // Host disconnect - start grace period
    if let Ok(Some(broadcast)) = state.broadcast.find_active_hosted_by(user_id).await {
        // Get disconnect count for tiered grace period
        let count_key = RedisKey::disconnect_count(broadcast.id);
        let disconnect_count: u64 = match state.redis.incr(&count_key).await {
            Ok(c) => c as u64,
            Err(_) => 1,
        };
        let _ = state.redis.expire(&count_key, TTL_3600_SECS).await;

        let config = crate::shared::services::ws::model::GracePeriodConfig::default();
        let grace_secs = config.get_grace_seconds(disconnect_count);

        // Set grace period key in Redis
        let grace_key = RedisKey::host_grace(broadcast.id);
        let value = &grace_secs.to_string();
        let _ = state.redis.set_ex(&grace_key, value, grace_secs).await;

        // Store grace start time
        let start_key = RedisKey::grace_started(broadcast.id);
        let value = &chrono::Utc::now().timestamp().to_string();
        let _ = state.redis.set_ex(&start_key, value, grace_secs + 10).await;

        if let Ok(participant_ids) = state.broadcast.get_participants_ids(broadcast.id).await {
            let payload = WsPayload::host_disconnected(broadcast.id, grace_secs, disconnect_count);
            state.ws.send_to_users(&participant_ids, payload).await;
        }

        // Spawn grace period watcher
        let state_clone = state.clone();
        let b_id = broadcast.id;
        let host_id = user_id;

        tokio::spawn(async move {
            tokio::time::sleep(time::Duration::from_secs(grace_secs)).await;

            // Check if grace key still exists (host didn't reconnect)
            let grace_key = RedisKey::host_grace(b_id);
            if state_clone.redis.exists(&grace_key).await.unwrap_or(false) {
                let _ = state_clone.redis.del(&grace_key).await;
                tracing::info!("Host grace expired for broadcast {}, ending", b_id);

                // End the broadcast via HTTP endpoint (handles all cleanups)
                let _ = state_clone
                    .broadcast
                    .end(&state_clone, b_id, host_id)
                    .await;
            }
        });
    }
}

/// Check reconnect rate limit to prevent DoS and crash loops
///
/// Sometimes, networks can cause rapid reconnect storms when the signal fluctuates.
/// This rate limiter prevents a single user from overwhelming the server.
///
/// Limits: 10 reconnects per 60 seconds → 30s backoff
async fn check_reconnect_rate(
    state: &MenoState,
    user_id: Uuid,
) -> Result<(), (StatusCode, Json<Value>)> {
    let key = RedisKey::reconnect_rate(user_id);

    let count: i64 = state.redis.incr(&key).await.map_err(|_| {
        error_response(StatusCode::INTERNAL_SERVER_ERROR, "Rate limit check failed")
    })?;

    if count == 1 {
        let _ = state.redis.expire(&key, 60).await;
    }

    if count > 10 {
        tracing::warn!("User {} reconnecting too fast ({}/60s)", user_id, count);
        return Err(error_response(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many reconnection attempts. Please wait 30 seconds.",
        ));
    }

    Ok(())
}

/// Start a periodic ping task
///
/// Pings every 25s to keep NAT bindings alive on mobile networks
fn start_heartbeat_task(
    ws_sender: Arc<Mutex<SplitSink<WebSocket, Message>>>,
    missed_pongs: Arc<AtomicU32>,
    config: HeartbeatConfig,
    user_id: Uuid,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = time::interval(time::Duration::from_secs(config.ping_interval_secs));
        loop {
            interval.tick().await;

            if missed_pongs.load(atomic::Ordering::Relaxed) >= config.max_missed_pings {
                tracing::warn!("User {} missed too many pongs, disconnecting", user_id);
                break;
            }

            let mut sender = ws_sender.lock().await;
            if sender.send(Message::Ping(vec![].into())).await.is_err() {
                tracing::warn!("Failed to send ping to user {}", user_id);
                break;
            }
        }
    })
}

/// Start task that writes outgoing messages to the WebSocket
fn start_write_task(
    ws_sender: Arc<Mutex<SplitSink<WebSocket, Message>>>,
    mut hub_rx: mpsc::Receiver<Arc<WsPayload>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut sender = ws_sender.lock().await;
        while let Some(payload) = hub_rx.recv().await {
            let json = serde_json::to_string(&payload).unwrap_or_default();
            if sender.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    })
}

/// Main read loop for incoming WebSocket messages
async fn run_read_loop(
    ws_receiver: &mut SplitStream<WebSocket>,
    state: &MenoState,
    user_id: Uuid,
    missed_pongs: &AtomicU32,
    pong_timeout: time::Duration,
    max_missed: u32,
) -> bool {
    let mut missed_count = 0;
    loop {
        let timeout = time::timeout(pong_timeout, ws_receiver.next()).await;
        match timeout {
            Ok(Some(Ok(Message::Text(text)))) => {
                missed_count = 0;
                missed_pongs.store(0, atomic::Ordering::Relaxed);
                handle_client_message(state, user_id, &text).await;
            }
            Ok(Some(Ok(Message::Ping(_)))) => {
                missed_count = 0;
                missed_pongs.store(0, atomic::Ordering::Relaxed);
            }
            Ok(Some(Ok(Message::Close(_))) | None) => {
                tracing::info!("WS closed for user {}", user_id);
                return false;
            }
            Ok(Some(Err(e))) => {
                tracing::warn!("WS error for user {}: {}", user_id, e);
                return true;
            }
            Err(_timeout) => {
                missed_count += 1;
                missed_pongs.fetch_add(1, atomic::Ordering::Relaxed);

                if missed_count >= max_missed {
                    tracing::warn!("WS timeout for user {} after {:?}", user_id, pong_timeout);
                    return true;
                }
            }
            _ => {}
        }
    }
}

fn error_response(code: StatusCode, message: &str) -> (StatusCode, Json<Value>) {
    (code, Json(json!({"error": message})))
}
