use crate::modules::chat::dto;
use crate::modules::chat::errors::ChatError;
use crate::shared::middleware::extractors::MenoQuery;
use crate::shared::pagination::CursorPage;
use crate::shared::services::ws::dto::WsErrorCode;
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::extract::State;
use std::sync::Arc;
use uuid::Uuid;
use validator::{Validate, ValidationErrors};

/// `GET /broadcasts/:id/chat/messages`
///
/// Cursor-paginated chat history.
///
/// This is the only HTTP endpoint in the chat
/// module; all mutations travel over WebSocket.
pub async fn get_messages(
    State(app): State<Arc<MenoState>>,
    MenoQuery(query): MenoQuery<dto::ChatMessageQuery>,
) -> Result<MenoResponse<CursorPage<dto::ChatMessageResponse>>, ChatError> {
    let page = app.chat.service.get_messages(&query).await?;
    Ok(MenoResponse::ok("Messages retrieved", page))
}

/// WS event: `sendMessage`
///
/// Expected client frame:
/// ```json
/// { "event": "sendMessage", "data": { "broadcastId": "…", "content": "…" } }
/// ```
/// `sender_id` is injected by the WS dispatcher from the authenticated claims —
/// the client never sends it.
pub async fn handle_ws_send_message(app: &MenoState, req: dto::SendMessageRequest) {
    if let Err(e) = req.validate() {
        handle_validation_error(app, req.sender_id, req.broadcast_id, e).await;
        return;
    }
    if let Err(err) = app.chat.service.send_message(app, &req).await {
        let code = match &err {
            ChatError::BroadcastNotLive => WsErrorCode::BroadcastForciblyEnded,
            ChatError::NotParticipant => WsErrorCode::KickedFromRoom,
            _ => WsErrorCode::Unsupported,
        };
        send_ws_error(app, req.sender_id, req.broadcast_id, code, err.to_string()).await;
    }
}

/// WS event: `editMessage`
///
/// Expected client frame:
/// ```json
/// { "event": "editMessage", "data": { "broadcastId": "…", "messageId": "…", "content": "…" } }
/// ```
pub async fn handle_ws_edit_message(app: &MenoState, req: dto::EditMessageRequest) {
    if let Err(e) = req.validate() {
        handle_validation_error(app, req.sender_id, req.broadcast_id, e).await;
        return;
    }
    if let Err(err) = app.chat.service.edit_message(app, &req).await {
        let code = match &err {
            ChatError::NotFound => WsErrorCode::Unsupported,
            ChatError::NotSender => WsErrorCode::KickedFromRoom,
            ChatError::EditWindowExpired => WsErrorCode::Unsupported,
            ChatError::BroadcastNotLive => WsErrorCode::BroadcastForciblyEnded,
            _ => WsErrorCode::Unsupported,
        };
        send_ws_error(app, req.sender_id, req.broadcast_id, code, err.to_string()).await;
    }
}

/// WS event: `deleteMessage`
///
/// Expected client frame:
/// ```json
/// { "event": "deleteMessage", "data": { "broadcastId": "…", "messageId": "…" } }
/// ```
pub async fn handle_ws_delete_message(app: &MenoState, req: dto::DeleteMessageRequest) {
    if let Err(err) = app.chat.service.delete_message(app, &req).await {
        let code = match &err {
            ChatError::NotSender => WsErrorCode::KickedFromRoom,
            _ => WsErrorCode::Unsupported,
        };
        send_ws_error(app, req.sender_id, req.broadcast_id, code, err.to_string()).await;
    }
}

/// WS event: `sendReaction`
///
/// Expected client frame:
/// ```json
/// { "event": "sendReaction", "data": { "broadcastId": "…", "content": "👏" } }
/// ```
pub async fn handle_ws_send_reaction(app: &MenoState, req: dto::SendReactionRequest) {
    if let Err(e) = req.validate() {
        handle_validation_error(app, req.sender_id, req.broadcast_id, e).await;
        return;
    }

    if let Err(err) = app.chat.service.send_reaction(app, &req).await {
        let code = match &err {
            ChatError::BroadcastNotLive => WsErrorCode::BroadcastForciblyEnded,
            ChatError::NotParticipant => WsErrorCode::KickedFromRoom,
            _ => WsErrorCode::Unsupported,
        };
        send_ws_error(app, req.sender_id, req.broadcast_id, code, err.to_string()).await;
    }
}

async fn send_ws_error(
    app: &MenoState,
    user_id: Uuid,
    broadcast_id: Uuid,
    code: WsErrorCode,
    msg: String,
) {
    app.ws.send_error(user_id, broadcast_id, code, msg).await;
}

async fn handle_validation_error(
    app: &MenoState,
    user_id: Uuid,
    broadcast_id: Uuid,
    error: ValidationErrors,
) {
    send_ws_error(
        app,
        user_id,
        broadcast_id,
        WsErrorCode::Unsupported,
        format!("Validation error: {:?}", error),
    )
    .await;
}
