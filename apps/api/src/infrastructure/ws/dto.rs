//! Wire types for the WebSocket surface: what the server sends, what it accepts,
//! and the constructors that keep those two in agreement.
//!
//! Ported from `apps/api/src/shared/services/ws/dto.rs` on `master` (`903c3ba`).
//!
//! # What changed
//!
//! - **No dependency on `modules::`.** `master`'s `WsPayload::ended_broadcast` took a
//!   concrete `broadcast::model::EndReason`, so this infrastructure module had to name
//!   a domain type that does not exist yet in the monorepo. It now accepts any
//!   `Serialize`, which keeps the envelope decoupled (plan §5.5's spirit applied to
//!   layering: infrastructure does not reach into modules) without losing the type
//!   safety of the *value* at the call site.
//! - **`error`/`unsupported_error` use the new [`WsError`] constructors**, so
//!   `recoverable` is derived from the code and connection-scoped errors omit
//!   `broadcastId` instead of sending `Uuid::nil()`.
//! - **Client message shapes are camelCase**, matching every other type in the API.
//!   On `master` they had no `rename_all`, so a client bound by the HTTP surface's
//!   conventions parsed them wrong.
//! - **Serialisation can no longer fail silently.** `master` used
//!   `serde_json::to_value(data).unwrap_or_default()`, which turns a serialisation bug
//!   into a frame containing `null` and no error anywhere. This keeps the fallback but
//!   logs it, so the failure is visible.

use super::errors::{WsError, WsErrorCode};
use super::model::WsEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
use validator::Validate;

/// A message sent from the server to a client.
///
/// The envelope is deliberately thin — `event` names what happened, `data` carries
/// whatever that event needs. A union type keyed by event would be nicer to consume
/// in a statically typed language, but it would also mean every new event is a breaking
/// change for the client's generated models; this shape lets both sides add events
/// without a coordinated release.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsPayload {
    /// What happened.
    pub event: WsEvent,
    /// Event-specific body.
    pub data: Value,
}

impl WsPayload {
    /// Build a payload from any serialisable body.
    ///
    /// A serialisation failure yields `data: null` rather than a panic — one broken
    /// payload must not take down a broadcast — but it is logged, because silently
    /// emitting `null` is how `master` lost errors.
    pub fn new(event: WsEvent, data: impl Serialize) -> Self {
        let data = serde_json::to_value(&data).unwrap_or_else(|e| {
            tracing::error!(
                error = %e,
                event = %event,
                "failed to serialise WS payload body; sending null"
            );
            Value::Null
        });
        Self { event, data }
    }

    /// A room-scoped error.
    #[must_use]
    pub fn error(broadcast_id: Uuid, code: WsErrorCode, message: impl Into<String>) -> Self {
        Self::new(
            WsEvent::BroadcastError,
            WsError::in_room(broadcast_id, code, message),
        )
    }

    /// An error scoped to the connection, with no broadcast behind it.
    ///
    /// `master` faked this by passing `Uuid::nil()`, which the client could not
    /// distinguish from a real room.
    #[must_use]
    pub fn connection_error(code: WsErrorCode, message: impl Into<String>) -> Self {
        Self::new(WsEvent::BroadcastError, WsError::connection(code, message))
    }

    /// The reply to a client event the server does not implement.
    #[must_use]
    pub fn unsupported_error(message: impl Into<String>) -> Self {
        Self::connection_error(WsErrorCode::Unsupported, message)
    }

    /// A direct-to-user notification.
    #[must_use]
    pub fn notification(user_id: Uuid, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self::new(
            WsEvent::Notification,
            serde_json::json!({
                "userId": user_id,
                "title": title.into(),
                "body": body.into(),
                "timestamp": time::OffsetDateTime::now_utc(),
            }),
        )
    }

    /// The host's connection dropped; `grace_secs` is how long the broadcast waits.
    ///
    /// The client shows a countdown and starts buffering rather than reconnecting
    /// immediately, because the connection *will* be accepted if it comes back in
    /// time.
    #[must_use]
    pub fn host_disconnected(broadcast_id: Uuid, grace_secs: u64, disconnect_count: u64) -> Self {
        Self::new(
            WsEvent::HostDisconnected,
            serde_json::json!({
                "broadcastId": broadcast_id,
                "gracePeriodInSecs": grace_secs,
                "disconnectCount": disconnect_count,
            }),
        )
    }

    /// The host returned inside the grace period; the broadcast continues.
    #[must_use]
    pub fn host_reconnected(broadcast_id: Uuid) -> Self {
        Self::new(
            WsEvent::HostReconnected,
            serde_json::json!({ "broadcastId": broadcast_id }),
        )
    }

    /// A participant joined. `participant` is a serialised projection.
    #[must_use]
    pub fn participant_joined(participant: impl Serialize) -> Self {
        Self::new(WsEvent::ParticipantJoined, participant)
    }

    /// A participant left.
    #[must_use]
    pub fn participant_left(participant: impl Serialize) -> Self {
        Self::new(WsEvent::ParticipantLeft, participant)
    }

    /// A participant was removed by the host.
    #[must_use]
    pub fn participant_kicked(participant: impl Serialize) -> Self {
        Self::new(WsEvent::ParticipantKicked, participant)
    }

    /// The live participant count changed.
    #[must_use]
    pub fn number_of_live_participants(broadcast_id: Uuid, count: i64) -> Self {
        Self::new(
            WsEvent::NumberOfLiveParticipants,
            serde_json::json!({ "broadcastId": broadcast_id, "count": count }),
        )
    }

    /// A broadcast went live.
    #[must_use]
    pub fn new_broadcast(broadcast: impl Serialize) -> Self {
        Self::new(WsEvent::NewBroadcast, broadcast)
    }

    /// A live broadcast ended.
    ///
    /// `reason` is any serialisable value — the domain's `EndReason` on `master`.
    /// Taking it as `Serialize` keeps this module free of a `modules::` dependency,
    /// which the monorepo layering does not allow.
    #[must_use]
    pub fn ended_broadcast(broadcast_id: Uuid, reason: impl Serialize) -> Self {
        Self::new(
            WsEvent::EndedBroadcast,
            serde_json::json!({ "broadcastId": broadcast_id, "reason": reason }),
        )
    }

    /// A cohost accepted their invitation.
    #[must_use]
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

    /// A cohost declined their invitation.
    #[must_use]
    pub fn cohost_declined(broadcast_id: Uuid, user_id: Uuid) -> Self {
        Self::new(
            WsEvent::CohostDeclined,
            serde_json::json!({ "broadcastId": broadcast_id, "userId": user_id }),
        )
    }

    /// Sent only to the invitee.
    #[must_use]
    pub fn cohost_invitation(broadcast_id: Uuid, token: String) -> Self {
        Self::new(
            WsEvent::CohostInvitation,
            serde_json::json!({ "broadcastId": broadcast_id, "token": token }),
        )
    }

    /// A cohost is back to being a plain participant; `token` re-authorises the client
    /// for the demoted role.
    #[must_use]
    pub fn cohost_demotion(broadcast_id: Uuid, token: String) -> Self {
        Self::new(
            WsEvent::CohostDemotion,
            serde_json::json!({ "broadcastId": broadcast_id, "token": token }),
        )
    }

    /// A cohost was removed from the broadcast.
    #[must_use]
    pub fn removed_cohost(cohost: impl Serialize) -> Self {
        Self::new(WsEvent::RemovedCohost, cohost)
    }

    /// The recording is playable.
    #[must_use]
    pub fn recording_ready(broadcast_id: Uuid) -> Self {
        Self::new(
            WsEvent::RecordingReady,
            serde_json::json!({ "broadcastId": broadcast_id }),
        )
    }

    /// A user's home screen is stale.
    #[must_use]
    pub fn home_invalidated() -> Self {
        Self::new(
            WsEvent::HomeInvalidated,
            serde_json::json!({ "timestamp": time::OffsetDateTime::now_utc() }),
        )
    }

    /// A chat message was created.
    #[must_use]
    pub fn new_message(data: impl Serialize) -> Self {
        Self::new(WsEvent::NewMessage, data)
    }

    /// A chat message was edited.
    #[must_use]
    pub fn edited_message(data: impl Serialize) -> Self {
        Self::new(WsEvent::EditedMessage, data)
    }

    /// A chat message was deleted.
    #[must_use]
    pub fn deleted_message(broadcast_id: Uuid, message_id: Uuid) -> Self {
        Self::new(
            WsEvent::DeletedMessage,
            serde_json::json!({ "messageId": message_id, "broadcastId": broadcast_id }),
        )
    }

    /// A reaction was added or removed.
    #[must_use]
    pub fn new_reaction(data: impl Serialize) -> Self {
        Self::new(WsEvent::NewReaction, data)
    }

    /// Announced on shutdown so clients can reconnect elsewhere before this process
    /// exits. `recoverable` is `true`, because reconnecting to another instance will
    /// work.
    ///
    /// Carries a `SERVER_SHUTDOWN` flag rather than an [`WsErrorCode`]: it is a
    /// transport signal rather than a domain error, and the client needs to tell it
    /// apart from a real failure so it reconnects instead of surfacing an alert.
    #[must_use]
    pub fn server_shutdown() -> Self {
        Self::new(
            WsEvent::BroadcastError,
            serde_json::json!({
                "code": "SERVER_SHUTDOWN",
                "message": "server is shutting down; reconnect to continue",
                "recoverable": true,
            }),
        )
    }
}

/// A frame received from a client.
///
/// `data` is left as raw JSON: the concrete shape depends on `event`, and the read
/// loop deserialises it into the matching request type once the event is known.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientMessage {
    /// Which client action this is.
    pub event: WsEvent,
    /// The action's arguments.
    pub data: Value,
}

/// The `?token=` query parameter on the upgrade request.
///
/// Serde rejects a missing field by default, which is what we want: an unauthenticated
/// upgrade is refused before a socket exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsQuery {
    /// The caller's access token.
    pub token: String,
}

/// The participant projection sent with join/leave/kick events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantWsResponseData {
    /// The broadcast the participant is in.
    pub broadcast_id: Uuid,
    /// The participant.
    pub user_id: Uuid,
    /// Display name.
    pub full_name: String,
    /// Optional profile bio. Omitted rather than null when absent, so the client
    /// does not have to distinguish "no bio" from "bio is the string null".
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bio: Option<String>,
    /// Optional avatar.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub avatar_url: Option<String>,
}

/// `editMessage` request body.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct WsEditMessage {
    /// Broadcast the message belongs to.
    pub broadcast_id: Uuid,
    /// Message to edit.
    pub message_id: Uuid,
    /// Replacement content. 1–256 characters, enforced by `Validate` so the bound is
    /// stated once next to the field rather than repeated in each handler.
    #[validate(length(min = 1, max = 256))]
    pub content: String,
}

/// `deleteMessage` request body.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct WsDeleteMessage {
    /// Broadcast the message belongs to.
    pub broadcast_id: Uuid,
    /// Message to delete.
    pub message_id: Uuid,
}

/// `sendReaction` request body.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct WsSendReaction {
    /// Broadcast to react in.
    pub broadcast_id: Uuid,
    /// Emoji or short label. 1–32 characters.
    #[validate(length(min = 1, max = 32))]
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn body(payload: &WsPayload) -> Value {
        serde_json::to_value(payload).expect("serialise")["data"].clone()
    }

    #[test]
    fn the_envelope_is_event_plus_data_in_camel_case() {
        let payload = WsPayload::host_reconnected(id(1));
        let json = serde_json::to_value(&payload).expect("serialise");

        assert_eq!(json["event"], "hostReconnected");
        assert_eq!(json["data"]["broadcastId"], id(1).to_string());
    }

    #[test]
    fn payload_builders_emit_camel_case_field_names() {
        // Every hand-written `json!` block is a chance to write snake_case by habit.
        // This walks the builders that build their body literally rather than from a
        // typed struct, because those are the ones no compiler checks.
        let cases: Vec<(&str, WsPayload, Vec<&str>)> = vec![
            (
                "host_disconnected",
                WsPayload::host_disconnected(id(1), 120, 2),
                vec!["broadcastId", "gracePeriodInSecs", "disconnectCount"],
            ),
            (
                "notification",
                WsPayload::notification(id(2), "t", "b"),
                vec!["userId", "title", "body", "timestamp"],
            ),
            (
                "number_of_live_participants",
                WsPayload::number_of_live_participants(id(3), 7),
                vec!["broadcastId", "count"],
            ),
            (
                "cohost_accepted",
                WsPayload::cohost_accepted(id(4), id(5), "Ada"),
                vec!["broadcastId", "userId", "userName"],
            ),
            (
                "cohost_declined",
                WsPayload::cohost_declined(id(4), id(5)),
                vec!["broadcastId", "userId"],
            ),
            (
                "deleted_message",
                WsPayload::deleted_message(id(4), id(6)),
                vec!["messageId", "broadcastId"],
            ),
        ];

        for (name, payload, expected) in cases {
            let data = body(&payload);
            for field in expected {
                assert!(
                    data.get(field).is_some(),
                    "{name} should emit camelCase `{field}`, got {data}"
                );
            }
            assert!(
                !data.to_string().contains('_'),
                "{name} leaked a snake_case key: {data}"
            );
        }
    }

    #[test]
    fn host_disconnected_carries_the_grace_window_the_client_counts_down() {
        let payload = WsPayload::host_disconnected(id(1), 120, 3);
        let data = body(&payload);
        assert_eq!(data["gracePeriodInSecs"], 120);
        assert_eq!(data["disconnectCount"], 3);
    }

    #[test]
    fn ended_broadcast_accepts_any_serialisable_reason() {
        // The point of taking `impl Serialize`: this module does not name a
        // `modules::` type, so the monorepo layering holds.
        #[derive(Serialize)]
        enum Reason {
            HostDisconnected,
        }
        let payload = WsPayload::ended_broadcast(id(1), Reason::HostDisconnected);
        let data = body(&payload);
        assert_eq!(data["broadcastId"], id(1).to_string());
        assert_eq!(data["reason"], "HostDisconnected");

        // A plain string works just as well.
        let payload = WsPayload::ended_broadcast(id(1), "host_disconnected");
        assert_eq!(body(&payload)["reason"], "host_disconnected");
    }

    #[test]
    fn room_errors_carry_the_broadcast_id_and_derived_recoverability() {
        let payload = WsPayload::error(id(1), WsErrorCode::TokenExpired, "expired");
        assert_eq!(payload.event, WsEvent::BroadcastError);

        let data = body(&payload);
        assert_eq!(data["broadcastId"], id(1).to_string());
        assert_eq!(data["code"], "TOKEN_EXPIRED");
        assert_eq!(data["message"], "expired");
        assert_eq!(data["recoverable"], true);
    }

    #[test]
    fn connection_errors_do_not_invent_a_broadcast_id() {
        let payload = WsPayload::unsupported_error("Unsupported event: endedBroadcast");
        let data = body(&payload);

        assert_eq!(data["code"], "UNSUPPORTED");
        assert_eq!(data["recoverable"], false);
        assert!(
            data.get("broadcastId").is_none(),
            "a connection-scoped error must not claim a room, got {data}"
        );
    }

    #[test]
    fn server_shutdown_is_recoverable_and_not_room_scoped() {
        // A client told the server is going away must reconnect rather than treat the
        // broadcast as finished.
        let payload = WsPayload::server_shutdown();
        let data = body(&payload);

        assert_eq!(payload.event, WsEvent::BroadcastError);
        assert_eq!(data["code"], "SERVER_SHUTDOWN");
        assert_eq!(data["recoverable"], true);
        assert!(data.get("broadcastId").is_none());
    }

    #[test]
    fn payloads_round_trip_for_the_pub_sub_bridge() {
        // The bridge serialises on one instance and deserialises on another; a
        // payload that does not survive a round trip is dropped mid-fan-out.
        for payload in [
            WsPayload::host_reconnected(id(1)),
            WsPayload::error(id(1), WsErrorCode::KickedFromRoom, "removed"),
            WsPayload::connection_error(WsErrorCode::MediaServerError, "livekit"),
            WsPayload::new_broadcast(serde_json::json!({ "id": id(2) })),
            WsPayload::server_shutdown(),
        ] {
            let json = serde_json::to_string(&payload).expect("serialise");
            let back: WsPayload = serde_json::from_str(&json).expect("deserialise");
            assert_eq!(back, payload, "round trip failed for {json}");
        }
    }

    #[test]
    fn client_messages_parse_in_camel_case() {
        let raw = r#"{"event":"sendMessage","data":{"broadcastId":"00000000-0000-0000-0000-000000000001","content":"hi"}}"#;
        let msg: ClientMessage = serde_json::from_str(raw).expect("deserialise");

        assert_eq!(msg.event, WsEvent::SendMessage);
        assert_eq!(msg.data["broadcastId"], id(1).to_string());
    }

    #[test]
    fn client_messages_reject_an_unknown_event() {
        // A frame naming an event that does not exist is a protocol error, not
        // something to silently coerce into a heartbeat.
        let raw = r#"{"event":"dropTables","data":{}}"#;
        assert!(serde_json::from_str::<ClientMessage>(raw).is_err());

        // Missing `data` is likewise rejected.
        assert!(serde_json::from_str::<ClientMessage>(r#"{"event":"heartbeat"}"#).is_err());
    }

    #[test]
    fn edit_message_bounds_the_content_length() {
        // The bound lives in the `Validate` attribute; this proves it is wired up.
        let ok = WsEditMessage {
            broadcast_id: id(1),
            message_id: id(2),
            content: "a".repeat(256),
        };
        assert!(validator::Validate::validate(&ok).is_ok());

        let empty = WsEditMessage {
            content: String::new(),
            ..ok.clone()
        };
        assert!(validator::Validate::validate(&empty).is_err());

        let too_long = WsEditMessage {
            content: "a".repeat(257),
            ..ok
        };
        assert!(validator::Validate::validate(&too_long).is_err());
    }

    #[test]
    fn send_reaction_bounds_the_content_length() {
        let ok = WsSendReaction {
            broadcast_id: id(1),
            content: "x".repeat(32),
        };
        assert!(validator::Validate::validate(&ok).is_ok());

        let too_long = WsSendReaction {
            content: "x".repeat(33),
            ..ok.clone()
        };
        assert!(validator::Validate::validate(&too_long).is_err());

        let empty = WsSendReaction {
            content: String::new(),
            ..ok
        };
        assert!(validator::Validate::validate(&empty).is_err());
    }

    #[test]
    fn edit_message_deserialises_from_the_camel_case_wire_shape() {
        let raw = r#"{
            "broadcastId": "00000000-0000-0000-0000-000000000001",
            "messageId":  "00000000-0000-0000-0000-000000000002",
            "content": "edited"
        }"#;
        let parsed: WsEditMessage = serde_json::from_str(raw).expect("deserialise");
        assert_eq!(parsed.broadcast_id, id(1));
        assert_eq!(parsed.message_id, id(2));
        assert_eq!(parsed.content, "edited");
    }

    #[test]
    fn participant_projection_round_trips() {
        let participant = ParticipantWsResponseData {
            broadcast_id: id(1),
            user_id: id(2),
            full_name: "Ada Lovelace".to_owned(),
            bio: None,
            avatar_url: Some("https://cdn.example/a.png".to_owned()),
        };
        let json = serde_json::to_value(&participant).expect("serialise");
        assert_eq!(json["fullName"], "Ada Lovelace");
        assert_eq!(json["avatarUrl"], "https://cdn.example/a.png");
        assert!(
            json.get("bio").is_none(),
            "an absent optional should be omitted, not null"
        );

        let back: ParticipantWsResponseData = serde_json::from_value(json).expect("deserialise");
        assert_eq!(back, participant);
    }

    #[test]
    fn ws_query_requires_a_token() {
        // An upgrade with no token must fail to parse, so the handler never sees a
        // socket it cannot authenticate.
        assert!(serde_json::from_str::<WsQuery>(r#"{}"#).is_err());
        assert!(serde_json::from_str::<WsQuery>(r#"{"token":"abc"}"#).is_ok());
    }
}
