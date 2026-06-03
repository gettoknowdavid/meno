use super::model::WsEvent;
use crate::modules::broadcast::model::EndReason;
pub(crate) use crate::shared::services::ws::errors::{WsError, WsErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

/// Generic WebSocket payload sent from server to client
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsPayload {
    pub event: WsEvent,
    pub data: Value,
}
impl WsPayload {
    pub fn new(event: WsEvent, data: impl Serialize) -> Self {
        let data = serde_json::to_value(data).unwrap_or_default();
        Self { event, data }
    }

    pub fn error(broadcast_id: Uuid, code: WsErrorCode, message: impl Into<String>) -> Self {
        let recoverable = code.clone().is_recoverable();
        Self::new(
            WsEvent::BroadcastError,
            WsError {
                broadcast_id,
                code,
                message: message.into(),
                recoverable,
                data: None,
            },
        )
    }

    pub fn notification(user_id: Uuid, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self::new(
            WsEvent::Notification,
            serde_json::json!({
                "userId": user_id,
                "title": title.into(),
                "body": body.into(),
                "timestamp": OffsetDateTime::now_utc(),
            }),
        )
    }

    pub fn host_disconnected(broadcast_id: Uuid, grace_period: u64, disconnect_count: u64) -> Self {
        Self::new(
            WsEvent::HostDisconnected,
            serde_json::json!({
                "broadcastId": broadcast_id,
                "gracePeriodInSecs": grace_period,
                "disconnectCount": disconnect_count,
            }),
        )
    }

    pub fn host_reconnected(broadcast_id: Uuid) -> Self {
        Self::new(
            WsEvent::HostDisconnected,
            serde_json::json!({"broadcastId": broadcast_id}),
        )
    }

    pub fn participant_joined(participant: impl Serialize) -> Self {
        Self::new(WsEvent::ParticipantJoined, participant)
    }

    pub fn participant_left(participant: impl Serialize) -> Self {
        Self::new(WsEvent::ParticipantJoined, participant)
    }

    pub fn participant_kicked(participant: impl Serialize) -> Self {
        Self::new(WsEvent::ParticipantKicked, participant)
    }

    pub fn number_of_live_participants(broadcast_id: Uuid, count: i64) -> Self {
        Self::new(
            WsEvent::NumberOfLiveParticipants,
            serde_json::json!({
                "broadcastId": broadcast_id,
                "count": count,
            }),
        )
    }

    pub fn new_broadcast(broadcast: impl Serialize) -> Self {
        Self::new(WsEvent::NewBroadcast, broadcast)
    }

    pub fn ended_broadcast(broadcast_id: Uuid, reason: EndReason) -> Self {
        Self::new(
            WsEvent::EndedBroadcast,
            serde_json::json!({
                "broadcastId": broadcast_id,
                "reason": reason,
            }),
        )
    }

    pub fn cohost_accepted(broadcast_id: Uuid, user_id: Uuid, user_name: &str) -> Self {
        Self::new(
            WsEvent::CohostAccepted,
            serde_json::json!({
                "broadcastId": broadcast_id,
                "userId": user_id,
                "userName": user_name,
            }),
        )
    }

    pub fn cohost_declined(broadcast_id: Uuid, user_id: Uuid) -> Self {
        Self::new(
            WsEvent::CohostDeclined,
            serde_json::json!({
                "broadcastId": broadcast_id,
                "userId": user_id,
            }),
        )
    }

    /// Emitted to the participant to whom the cohost invite was sent
    pub fn cohost_invitation(broadcast_id: Uuid, token: String) -> Self {
        Self::new(
            WsEvent::NewCohost,
            serde_json::json!({
                "broadcastId": broadcast_id,
                "token": token,
            }),
        )
    }

    pub fn cohost_demotion(broadcast_id: Uuid, token: String) -> Self {
        Self::new(
            WsEvent::CohostDemotion,
            serde_json::json!({
                "broadcastId": broadcast_id,
                "token": token,
            }),
        )
    }

    pub fn removed_cohost(cohost: impl Serialize) -> Self {
        Self::new(WsEvent::RemovedCohost, cohost)
    }

    pub fn recording_ready(broadcast_id: Uuid) -> Self {
        Self::new(
            WsEvent::RecordingReady,
            serde_json::json!({ "broadcastId": broadcast_id }),
        )
    }

    pub fn home_invalidated() -> Self {
        Self::new(
            WsEvent::HomeInvalidated,
            serde_json::json!({ "timestamp": OffsetDateTime::now_utc() }),
        )
    }
}

/// Client message received from WebSocket
/// Uses custom deserialization to convert string event to WsEvent enum
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientMessage {
    pub event: WsEvent,
    pub data: Value,
}

#[derive(Debug, serde::Deserialize)]
pub struct WsQuery {
    pub token: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantWsResponseData {
    pub broadcast_id: Uuid,
    pub user_id: Uuid,
    pub full_name: String,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
}
