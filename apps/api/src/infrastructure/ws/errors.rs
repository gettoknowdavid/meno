//! WebSocket error payloads.
//!
//! Ported from `apps/api/src/shared/services/ws/errors.rs` on `master` (`903c3ba`).
//!
//! # What changed, and why
//!
//! - **`broadcast_id` is optional.** On `master` the field was a mandatory `Uuid`, so
//!   the only way to report an error with no broadcast behind it — a malformed
//!   heartbeat, an unsupported event, server shutdown — was to pass `Uuid::nil()`,
//!   which serialises as `"00000000-0000-0000-0000-000000000000"` and reads on the
//!   client as a real room. `Option<Uuid>` with `skip_serializing_if` says "not
//!   room-scoped" honestly.
//! - **Recoverability is derived, not stored.** `master` computed `recoverable` from
//!   the code at construction and also shipped a `is_recoverable` method. Now there is
//!   one source, and the field cannot disagree with the code it was built from.
//! - **Codes align with `meno_core::ErrorCode` casing.** `SCREAMING_SNAKE_CASE`, like
//!   the HTTP surface, so a client has one convention for codes across both.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A structured error delivered over the socket.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsError {
    /// The broadcast this error concerns, when it concerns one.
    ///
    /// Absent for connection-scoped errors that belong to no room.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub broadcast_id: Option<Uuid>,

    /// The stable code. Clients branch on this, never on [`Self::message`].
    pub code: WsErrorCode,

    /// Human-readable detail. Safe to show, never used for control flow.
    pub message: String,

    /// Whether retrying the same action can succeed.
    pub recoverable: bool,

    /// Optional structured detail, for codes that carry extra context.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub data: Option<serde_json::Value>,
}

impl WsError {
    /// An error scoped to a broadcast room.
    ///
    /// `recoverable` is taken from [`WsErrorCode::is_recoverable`] rather than passed
    /// in, so it cannot be set to a value that contradicts the code.
    #[must_use]
    pub fn in_room(broadcast_id: Uuid, code: WsErrorCode, message: impl Into<String>) -> Self {
        Self {
            broadcast_id: Some(broadcast_id),
            code,
            message: message.into(),
            recoverable: code.is_recoverable(),
            data: None,
        }
    }

    /// An error scoped to the connection, not to any broadcast.
    #[must_use]
    pub fn connection(code: WsErrorCode, message: impl Into<String>) -> Self {
        Self {
            broadcast_id: None,
            code,
            message: message.into(),
            recoverable: code.is_recoverable(),
            data: None,
        }
    }

    /// Attach structured detail.
    #[must_use]
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

/// Why a WebSocket operation failed.
///
/// The `is_recoverable` doc comments are the contract the client's reconnect logic
/// depends on, so they are stated per variant rather than left to be inferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WsErrorCode {
    /// The room token passed its expiry.
    ///
    /// Recoverable: the client re-fetches a token from `/broadcasts/:id/token` and
    /// reconnects.
    TokenExpired,

    /// An administrator ended the broadcast.
    ///
    /// Not recoverable. Retrying is pointless and the room is gone for good.
    BroadcastForciblyEnded,

    /// The host removed this participant from the room.
    ///
    /// Not recoverable. Rejoining would be circumventing the host's decision.
    KickedFromRoom,

    /// The broadcast room does not exist, or has already ended.
    ///
    /// Not recoverable for this client; a different broadcast may still be joinable.
    RoomNotFound,

    /// The media server was unreachable.
    ///
    /// Recoverable: this is the dependency most likely to come back on its own.
    MediaServerError,

    /// The client sent an event or payload the server does not handle.
    ///
    /// Not recoverable by retrying the same frame — but not fatal either, so the
    /// socket stays open and only this frame is dropped.
    Unsupported,
}

impl WsErrorCode {
    /// Whether retrying can succeed.
    ///
    /// The single source of truth: [`WsError::recoverable`] is filled from this
    /// rather than computed at each call site, so the flag on the wire can never
    /// contradict the code it accompanies.
    #[must_use]
    pub const fn is_recoverable(self) -> bool {
        matches!(self, Self::TokenExpired | Self::MediaServerError)
    }

    /// Whether this error means the socket itself should be closed.
    ///
    /// Distinct from `is_recoverable`: an unsupported frame is not recoverable by
    /// retrying, but it also is not a reason to drop a working connection.
    #[must_use]
    pub const fn is_fatal(self) -> bool {
        matches!(
            self,
            Self::BroadcastForciblyEnded | Self::KickedFromRoom | Self::RoomNotFound
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn recoverability_is_derived_from_the_code() {
        // The invariant that `master` could violate by hand: a TOKEN_EXPIRED error
        // marked non-recoverable would leave a client never retrying a reconnect it
        // could have made.
        let recoverable = WsErrorCode::TokenExpired;
        let fatal = WsErrorCode::KickedFromRoom;

        assert!(recoverable.is_recoverable());
        assert!(!fatal.is_recoverable());

        let err = WsError::in_room(id(1), recoverable, "expired");
        assert!(err.recoverable);
        assert_eq!(err.recoverable, err.code.is_recoverable());

        let err = WsError::in_room(id(1), fatal, "kicked");
        assert!(!err.recoverable);
        assert_eq!(err.recoverable, err.code.is_recoverable());
    }

    #[test]
    fn fatal_and_recoverable_are_disjoint() {
        // A client deciding "close the socket" must never also be told "retry this".
        // Unsupported is the interesting case: not recoverable, not fatal.
        for code in [
            WsErrorCode::TokenExpired,
            WsErrorCode::BroadcastForciblyEnded,
            WsErrorCode::KickedFromRoom,
            WsErrorCode::RoomNotFound,
            WsErrorCode::MediaServerError,
            WsErrorCode::Unsupported,
        ] {
            assert!(
                !(code.is_fatal() && code.is_recoverable()),
                "{code:?} is both fatal and recoverable; a client cannot act on that"
            );
        }

        assert!(WsErrorCode::RoomNotFound.is_fatal());
        assert!(WsErrorCode::KickedFromRoom.is_fatal());
        assert!(WsErrorCode::BroadcastForciblyEnded.is_fatal());
        assert!(!WsErrorCode::Unsupported.is_fatal());
        assert!(!WsErrorCode::TokenExpired.is_fatal());
    }

    #[test]
    fn connection_scoped_errors_omit_the_broadcast_id() {
        // `master` sent Uuid::nil() here, which serialises as an all-zero UUID and
        // reads on the client as a genuine room.
        let err = WsError::connection(WsErrorCode::Unsupported, "nope");
        let json = serde_json::to_value(&err).expect("serialise");

        assert!(
            json.get("broadcastId").is_none(),
            "a connection-scoped error must not claim a broadcast"
        );
        assert_eq!(json["code"], "UNSUPPORTED");
        assert_eq!(json["recoverable"], false);
    }

    #[test]
    fn room_scoped_errors_carry_the_broadcast_id_in_camel_case() {
        let err = WsError::in_room(id(2), WsErrorCode::TokenExpired, "token expired");
        let json = serde_json::to_value(&err).expect("serialise");

        assert_eq!(json["broadcastId"], id(2).to_string());
        assert_eq!(json["code"], "TOKEN_EXPIRED");
        assert_eq!(json["recoverable"], true);
        assert!(
            json.get("data").is_none(),
            "an absent data field should be omitted, not null"
        );
    }

    #[test]
    fn with_data_attaches_detail_without_disturbing_the_rest() {
        let err = WsError::connection(WsErrorCode::Unsupported, "bad frame")
            .with_data(serde_json::json!({"field": "content"}));

        let json = serde_json::to_value(&err).expect("serialise");
        assert_eq!(json["data"]["field"], "content");
        assert_eq!(json["code"], "UNSUPPORTED");
        assert_eq!(json["message"], "bad frame");
    }

    #[test]
    fn errors_round_trip_through_serde() {
        // The pub/sub bridge deserialises envelopes on the receiving instance, so a
        // payload that does not survive a round trip is dropped mid-fan-out.
        for err in [
            WsError::in_room(id(3), WsErrorCode::RoomNotFound, "gone"),
            WsError::connection(WsErrorCode::MediaServerError, "livekit down"),
        ] {
            let json = serde_json::to_string(&err).expect("serialise");
            let back: WsError = serde_json::from_str(&json).expect("deserialise");
            assert_eq!(back, err);
        }
    }

    #[test]
    fn code_names_match_the_http_taxonomy_casing() {
        // One casing convention across both surfaces, so a Dart client has a single
        // rule for matching a code.
        let codes = [
            (WsErrorCode::TokenExpired, "TOKEN_EXPIRED"),
            (
                WsErrorCode::BroadcastForciblyEnded,
                "BROADCAST_FORCIBLY_ENDED",
            ),
            (WsErrorCode::KickedFromRoom, "KICKED_FROM_ROOM"),
            (WsErrorCode::RoomNotFound, "ROOM_NOT_FOUND"),
            (WsErrorCode::MediaServerError, "MEDIA_SERVER_ERROR"),
            (WsErrorCode::Unsupported, "UNSUPPORTED"),
        ];
        for (code, name) in codes {
            assert_eq!(
                serde_json::to_string(&code).expect("serialise"),
                format!("\"{name}\""),
                "{name} should be SCREAMING_SNAKE_CASE"
            );
        }
    }
}
