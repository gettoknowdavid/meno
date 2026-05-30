use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsError {
    pub broadcast_id: Uuid,

    pub code: WsErrorCode,

    pub message: String,

    /// If `true`, then a retry is possible
    pub recoverable: bool,

    pub data: Option<serde_json::Value>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WsErrorCode {
    /// [BroadcastWsError.recoverable] = `true`
    ///
    /// Retry is possible, so call the `/broadcasts/:id/token`
    TokenExpired,

    /// [BroadcastWsError.recoverable] = `false`
    ///
    /// Admin has ended the broadcast.
    BroadcastForciblyEnded,

    /// [BroadcastWsError.recoverable] = `false`
    ///
    /// Host has removed a participant from the room
    KickedFromRoom,

    /// [BroadcastWsError.recoverable] = `false`
    ///
    /// Broadcast room wasn't found
    RoomNotFound,

    /// [BroadcastWsError.recoverable] = `true`
    ///
    /// Retry the connection
    MediaServerError,

    Unsupported,
}

impl WsErrorCode {
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            WsErrorCode::TokenExpired | WsErrorCode::MediaServerError
        )
    }
}
