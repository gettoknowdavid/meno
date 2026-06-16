use crate::modules::chat::dto;
use crate::modules::chat::errors::ChatError;
use crate::shared::middleware::extractors::MenoQuery;
use crate::shared::pagination::CursorPage;
use crate::shared::services::ws::dto::{WsErrorCode, WsPayload};
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::extract::State;
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

pub async fn get_messages(
    State(app): State<Arc<MenoState>>,
    MenoQuery(query): MenoQuery<dto::ChatMessageQuery>,
) -> Result<MenoResponse<CursorPage<dto::ChatMessageResponse>>, ChatError> {
    let page = app.chat.service.get_messages(&query).await?;
    Ok(MenoResponse::ok("Messages retrieved", page))
}

// #################### WEB-SOCKET HANDLERS ####################
pub async fn handle_ws_send_message(app: &MenoState, req: dto::SendMessageRequest) {
    let broadcast_id = req.broadcast_id;
    let sender_id = req.sender_id;

    if let Err(e) = req.validate() {
        let msg = format!("Validation error: {:?}", e);
        send_ws_error(&app, sender_id, Uuid::nil(), WsErrorCode::Unsupported, msg).await;
        return;
    }

    match app.chat.service.send_message(&req).await {
        Err(err) => {
            let code = match &err {
                ChatError::BroadcastNotLive => WsErrorCode::BroadcastForciblyEnded,
                ChatError::NotParticipant => WsErrorCode::KickedFromRoom,
                _ => WsErrorCode::Unsupported,
            };
            send_ws_error(&app, sender_id, broadcast_id, code, err.to_string()).await;
        }
        Ok(response) => {
            let payload = WsPayload::new_message(response);

            // let b_id = req.broadcast_id.clone();
            // let response_clone = response.clone();
            // let broadcast_service = app.broadcast.service.clone();
            // let ws = self.ws.clone();
            // tokio::spawn(async move {
            //     let payload = WsPayload::new_message(response_clone);
            //     if let Ok(ids) = broadcast_service.get_participants_ids(b_id).await {
            //         ws.send_to_users(&ids, payload).await;
            //     }
            // });
        },
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
