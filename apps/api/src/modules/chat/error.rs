//! Chat's contribution to the single error taxonomy (plan §4.2).
//!
//! No `ChatError` enum: the port replaces `master`'s eleven-variant enum and its
//! hundred-line `IntoResponse` with named constructors over [`MenoError`], the same
//! shape as [`crate::modules::broadcast::error`] and
//! [`crate::modules::settings::error`]. The codes that replace `master`'s
//! `MESSAGE_NOT_FOUND`, `NOT_MESSAGE_SENDER`, `BROADCAST_NOT_LIVE`,
//! `NOT_BROADCAST_PARTICIPANT`, `EDIT_WINDOW_EXPIRED` and friends are shared taxonomy
//! entries, so no client loses a state it could branch on.
//!
//! # Three variants of `master`'s enum that are gone
//!
//! `BroadcastNotFound`, `UserNotFound` and `CannotSendMessageToSelf` were declared and
//! never constructed — `grep` over `master`'s chat module finds only the enum. A
//! constructor nobody calls is a branch nobody tests, and a client that branches on its
//! code is branching on something that cannot happen.

use meno_core::{Error as MenoError, ErrorCode};

use crate::infrastructure::ws::errors::WsErrorCode;

/// No live message carries this id, or it has been deleted.
///
/// A soft-deleted message is *not found* rather than *gone*: the row still exists, but
/// nothing that reads chat can see it, so the distinction is invisible to a client and
/// inventing a `deleted` state for it would be a code nobody branches on.
#[must_use]
pub fn message_not_found() -> MenoError {
    MenoError::NotFound {
        resource: "message",
        code: ErrorCode::NotFound,
    }
}

/// The broadcast exists but is not live, so nobody may post in it.
///
/// A `409` rather than a `400`, matching [`crate::modules::broadcast::error::not_live`]
/// for the same state: the request is well-formed and the resource's state is what
/// refuses it.
#[must_use]
pub fn broadcast_not_live() -> MenoError {
    MenoError::Conflict {
        code: ErrorCode::Conflict,
        message: "This broadcast is not live".to_owned(),
    }
}

/// The caller is not in the room, so they may not read or write its chat.
#[must_use]
pub fn not_participant() -> MenoError {
    MenoError::Forbidden {
        code: ErrorCode::NotParticipant,
        message: "You are not in this broadcast".to_owned(),
    }
}

/// The message belongs to somebody else.
///
/// Deliberately distinct from [`message_not_found`]: a client that mixes the two tells
/// its user \"that message is gone\" when the truth is \"that is not yours\", and the
/// second is actionable.
#[must_use]
pub fn not_sender() -> MenoError {
    MenoError::Forbidden {
        code: ErrorCode::Forbidden,
        message: "You can only change your own messages".to_owned(),
    }
}

/// The edit window closed before the edit arrived.
///
/// A `400` as `master` answered it. The window itself lives in
/// [`super::model::EDIT_WINDOW_SECONDS`], so this message and that constant are allowed
/// to drift only in wording.
#[must_use]
pub fn edit_window_expired() -> MenoError {
    MenoError::BadRequest {
        code: ErrorCode::BadRequest,
        message: "This message can no longer be edited".to_owned(),
    }
}

/// The socket-level code that explains a refusal to the client that sent the frame.
///
/// The HTTP taxonomy ([`MenoError`]) and the socket taxonomy ([`WsErrorCode`]) are not
/// the same vocabulary: a socket frame has no status code, and the client's reaction to
/// a message it may not edit is not the same as its reaction to a room it is no longer
/// in. So the mapping is explicit rather than inferred from a status.
///
/// # The two codes that are *fatal*, and why only two states get them
///
/// `BroadcastForciblyEnded` and `KickedFromRoom` both tell a client to close the socket
/// ([`WsErrorCode::is_fatal`]), so they are reserved for the states where the room really
/// is gone: the broadcast has ended, or the caller is not a participant. Everything else
/// — someone else's message, a closed edit window, a message that no longer exists — is
/// `Unsupported`: not recoverable by retrying, but not a reason to drop a working
/// connection either.
///
/// That last distinction is a fix. `master` mapped *every* failed `deleteMessage` to
/// `KickedFromRoom`, so a client deleting a message it had already deleted was told it
/// had been removed from the room — and its reconnect logic believed it.
#[must_use]
pub fn ws_code(failure: &MenoError) -> WsErrorCode {
    match failure {
        MenoError::Conflict { .. } => WsErrorCode::BroadcastForciblyEnded,
        MenoError::Forbidden {
            code: ErrorCode::NotParticipant,
            ..
        } => WsErrorCode::KickedFromRoom,
        _ => WsErrorCode::Unsupported,
    }
}

/// A driver failure inside the chat module. Always `500`, always logged.
///
/// `context` names the operation and is never user input; `detail` is the driver's own
/// text, which reaches the log and never the client.
#[must_use]
pub fn internal(context: &'static str, detail: impl std::fmt::Display) -> MenoError {
    MenoError::Internal {
        context,
        detail: detail.to_string(),
    }
}

#[cfg(test)]
mod tests {
    //! §4.2's load-bearing properties, asserted on the rendered body.

    use super::*;
    use meno_core::to_body;

    #[test]
    fn every_code_is_one_the_taxonomy_defines() {
        assert_eq!(message_not_found().code(), ErrorCode::NotFound);
        assert_eq!(not_participant().code(), ErrorCode::NotParticipant);
        assert_eq!(not_sender().code(), ErrorCode::Forbidden);
        assert_eq!(broadcast_not_live().code(), ErrorCode::Conflict);
        assert_eq!(edit_window_expired().code(), ErrorCode::BadRequest);
    }

    #[test]
    fn a_refusal_is_a_4xx() {
        assert_eq!(to_body(&message_not_found()).http_status, 404);
        assert_eq!(to_body(&not_participant()).http_status, 403);
        assert_eq!(to_body(&not_sender()).http_status, 403);
        assert_eq!(to_body(&broadcast_not_live()).http_status, 409);
        assert_eq!(to_body(&edit_window_expired()).http_status, 400);
    }

    #[test]
    fn a_refusal_is_client_safe_and_an_internal_failure_is_not() {
        assert!(not_participant().is_client_safe());
        assert!(!internal("send_message", "connection refused").is_client_safe());
    }

    #[test]
    fn internal_detail_is_logged_never_returned() {
        let error = internal(
            "insert_message",
            "relation \"chat_messages\" does not exist",
        );

        assert_eq!(to_body(&error).http_status, 500);
        assert!(
            !to_body(&error).message.contains("chat_messages"),
            "the driver text leaked: {}",
            to_body(&error).message
        );
        assert!(error.to_string().contains("insert_message"));
    }

    #[test]
    fn only_the_states_that_end_the_room_map_to_a_fatal_socket_code() {
        // The invariant the client's reconnect logic depends on: a refusal it can retry
        // differently must not be delivered as "this room is over".
        assert_eq!(
            ws_code(&broadcast_not_live()),
            WsErrorCode::BroadcastForciblyEnded
        );
        assert_eq!(ws_code(&not_participant()), WsErrorCode::KickedFromRoom);
        assert!(ws_code(&broadcast_not_live()).is_fatal());
        assert!(ws_code(&not_participant()).is_fatal());

        // Each of these keeps the socket open: the frame failed, the connection is fine.
        for refusal in [not_sender(), edit_window_expired(), message_not_found()] {
            assert_eq!(ws_code(&refusal), WsErrorCode::Unsupported);
            assert!(!ws_code(&refusal).is_fatal());
            assert!(!ws_code(&refusal).is_recoverable());
        }
    }

    #[test]
    fn a_driver_failure_is_not_reported_as_a_room_state() {
        // A 500 must not tell the client its broadcast ended: that is an answer the
        // client acts on by leaving.
        let failure = internal("insert_message", "connection refused");

        assert_eq!(ws_code(&failure), WsErrorCode::Unsupported);
        assert!(!ws_code(&failure).is_fatal());
    }

    #[test]
    fn a_missing_message_and_somebody_elses_message_are_different_answers() {
        // Same status family, different code and different sentence: one is \"gone\",
        // the other is \"not yours\", and only the second is actionable.
        assert_ne!(message_not_found().code(), not_sender().code());
        assert_ne!(
            to_body(&message_not_found()).message,
            to_body(&not_sender()).message
        );
    }
}
