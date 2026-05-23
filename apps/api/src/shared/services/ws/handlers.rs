use crate::modules::auth::model::User;
use crate::shared::services::redis::RedisService;
use crate::shared::services::ws::hub::WsPayload;
use crate::state::MenoState;
use axum::{
    Json,
    extract::ws::{Message, WebSocket},
    extract::{Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::Response,
};
use futures_util::{SinkExt, stream::StreamExt};
use serde_json::json;
use std::sync::{Arc, atomic};
use tokio::{sync::mpsc, time};
use uuid::Uuid;

#[derive(Debug, serde::Deserialize)]
pub struct WsQuery {
    pub token: String,
}

#[derive(Debug, serde::Deserialize)]
struct ClientMessage {
    event: String,
    data: serde_json::Value,
}

// GET /ws?token=<access_jwt>
pub async fn ws_upgrade(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<Arc<MenoState>>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let claims = state
        .jwt
        .decode_access(&query.token)
        .map_err(|_| error_response(StatusCode::UNAUTHORIZED, "Invalid token"))?;

    let user = state
        .auth_service
        .find_user_by_id(claims.sub)
        .await
        .map_err(|_| error_response(StatusCode::BAD_REQUEST, "User not found"))?
        .ok_or(error_response(StatusCode::BAD_REQUEST, "User not found"))?;

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, user, state)))
}

async fn handle_socket(socket: WebSocket, user: User, state: Arc<MenoState>) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let (hub_tx, hub_rx) = mpsc::channel::<Arc<WsPayload>>(128);
    let conn_id = state.ws.register(user.id, hub_tx.clone());

    // Spawn the write task
    let write_task = tokio::spawn(async move {
        let mut ws_sender = ws_sender;
        let mut hub_rx = hub_rx;
        while let Some(payload) = hub_rx.recv().await {
            let json = serde_json::to_string(&payload).unwrap_or_default();
            let json_bytes = axum::extract::ws::Utf8Bytes::from(json);
            if ws_sender.send(Message::Text(json_bytes)).await.is_err() {
                break;
            }
        }
    });

    // Heartbeat + Read loop
    let mut heartbeat_missed = Arc::new(atomic::AtomicU32::new(0));
    let missed_clone = heartbeat_missed.clone();

    // Simple heartbeat task (25s ping)
    let heartbeat_task = tokio::spawn(async move {
        let mut interval = time::interval(tokio::time::Duration::from_secs(25));
        loop {
            interval.tick().await;
            if missed_clone.fetch_add(1, atomic::Ordering::Relaxed) >= 2 {
                break;
            }
        }
    });

    // Main read loop
    loop {
        let timeout = time::timeout(time::Duration::from_secs(60), ws_receiver.next()).await;
        match timeout {
            Ok(Some(Ok(Message::Text(text)))) => {
                heartbeat_missed.store(0, atomic::Ordering::Relaxed);
                handle_client_message(&text, user.id, state.clone()).await;
            }
            Ok(Some(Ok(Message::Ping(_)))) => {
                heartbeat_missed.store(0, atomic::Ordering::Relaxed);
            }
            Ok(Some(Ok(Message::Close(_))) | None) => break,
            Ok(Some(Err(e))) => {
                tracing::warn!("WS error: {}", e);
                break;
            }
            Err(e) => {
                tracing::warn!("WS timeout error: {}", e);
                break;
            }
            _ => {}
        }
    }

    heartbeat_task.abort();
    write_task.abort();
    state.ws.unregister(user.id, conn_id);

    if state.ws.connection_count(user.id) == 0 {
        let key = RedisService::presence_key(user.id);
        let _ = state.redis.del(&key).await.map_err(|e| {
            tracing::error!("Redis Error: {}", e);
        });

        // todo("Handle broadcast clean up")!
    }
}

pub async fn handle_client_message(raw_text: &str, user_id: Uuid, state: Arc<MenoState>) {
    let msg: ClientMessage = match serde_json::from_str(&raw_text) {
        Ok(m) => m,
        Err(_) => {
            tracing::warn!("Invalid WS message from user {}", user_id);
            return;
        }
    };
    match msg.event.as_str() {
        "endBroadcast" => {}
        "leaveBroadcast" => {}
        _ => tracing::warn!("Unknown WS event: {}", msg.event),
    }
}
fn error_response(code: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (code, Json(json!({"error": message})))
}
