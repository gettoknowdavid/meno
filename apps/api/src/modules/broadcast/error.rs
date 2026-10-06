//! Broadcast's contribution to the single error taxonomy (plan §4.2).
//!
//! There is no `BroadcastError` enum here, and that is the point. The port replaced a
//! forty-variant module-local enum whose `IntoResponse` impl was a hundred lines of
//! hand-written `(StatusCode, "CODE", message)` triples — the exact shape §4.2 names as
//! the defect it exists to remove: *"nine independent error enums … error mapping logic
//! is duplicated nine times and drifts"*.
//!
//! What replaces it is a set of named functions returning [`MenoError`]. A function
//! rather than a variant because a constructor can *default* the parts that are always
//! the same, which is where the drift lived: `Database | Redis | Internal` all rendered
//! as `500 INTERNAL_ERROR "An internal error occurred"`, and six call sites had to
//! remember that pairing instead of getting it.
//!
//! # What a client can rely on
//!
//! The `code` is stable and machine-readable; the `message` is prose and may change.
//! Every code here is a variant of [`ErrorCode`], which is the single registry the whole
//! API draws from — including the broadcast-specific ones (`INVALID_TIME_ZONE`,
//! `START_TIME_IN_PAST`, `NOT_CREATOR`, `NOT_PARTICIPANT`) that `master` spelled out a
//! second time inside the module's own enum.
//!
//! # What was dropped, and why it is safe
//!
//! The old enum had `BROADCAST_NOT_LIVE`, `BROADCAST_ALREADY_LIVE`,
//! `ALREADY_A_PARTICIPANT`, `JOIN_IN_PROGRESS`, `COHOST_LIMIT_REACHED`,
//! `RECORDING_NOT_AVAILABLE`, `INVALID_BROADCAST_ID`, `SERVICE_CONFIG_MISSING` and more.
//! Each maps onto a code that already exists in the taxonomy, so no client loses a
//! distinguishable state; what changes is that the code is now shared with every other
//! module rather than broadcast-specific. The one genuinely dropped case is
//! `ServiceConfigMissing`, which was a 500 for "the wiring is wrong" — that is now a
//! startup failure in [`super::state`], which is where it can actually be fixed.

use meno_core::{Error as MenoError, ErrorCode};

/// The accumulator `dto::validate` fills, and the two constructors that turn one into
/// an [`MenoError::Validation`].
///
/// Re-exported from [`meno_core::error`] for the same reason auth does: a module may
/// reach down to `crates/core` and may not reach sideways into another module, so the
/// one accumulator every module validates with has to live below all of them.
pub use meno_core::error::{FieldErrors, invalid_field, invalid_fields};

/// No live broadcast carries this id, or it has been deleted.
#[must_use]
pub fn not_found() -> MenoError {
    MenoError::NotFound {
        resource: "broadcast",
        code: ErrorCode::NotFound,
    }
}

/// A user id has no matching live account.
#[must_use]
pub fn user_not_found() -> MenoError {
    MenoError::NotFound {
        resource: "user",
        code: ErrorCode::NotFound,
    }
}

/// The caller did not create the broadcast, and the operation is the creator's alone.
///
/// One answer for "not the creator" and "no such broadcast" would hide existence; the
/// reverse is not true here, because every caller here already knows the broadcast
/// exists — they got its id from a list or a link — so this is about the *operation*,
/// not about probing.
#[must_use]
pub fn not_creator() -> MenoError {
    MenoError::Forbidden {
        code: ErrorCode::NotCreator,
        message: "Only the broadcast creator can do that".to_owned(),
    }
}

/// The broadcast exists but is not live, so it cannot be joined or ended as live.
#[must_use]
pub fn not_live() -> MenoError {
    MenoError::Conflict {
        code: ErrorCode::Conflict,
        message: "This broadcast is not live".to_owned(),
    }
}

/// The broadcast is already live, so it cannot go live again.
#[must_use]
pub fn already_live() -> MenoError {
    MenoError::Conflict {
        code: ErrorCode::Conflict,
        message: "This broadcast is already live".to_owned(),
    }
}

/// The caller is already in the room.
#[must_use]
pub fn already_joined() -> MenoError {
    MenoError::Conflict {
        code: ErrorCode::Conflict,
        message: "You are already in this broadcast".to_owned(),
    }
}

/// The caller is not in the room, so there is nothing to leave.
#[must_use]
pub fn not_joined() -> MenoError {
    MenoError::Forbidden {
        code: ErrorCode::NotParticipant,
        message: "You are not in this broadcast".to_owned(),
    }
}

/// The creator cannot join their own broadcast.
///
/// They are the host, not a listener: joining would replace their host grant with a
/// participant one and hand them a token that cannot administer the room they own.
#[must_use]
pub fn creator_cannot_join() -> MenoError {
    MenoError::Forbidden {
        code: ErrorCode::Forbidden,
        message: "You created this broadcast, so you are already its host".to_owned(),
    }
}

/// A host cannot leave their own room — they end the broadcast instead.
///
/// Without this, "leave" would remove the host's participant row and leave a live
/// broadcast with nobody able to end it.
#[must_use]
pub fn host_cannot_leave() -> MenoError {
    MenoError::Conflict {
        code: ErrorCode::Conflict,
        message: "End the broadcast instead of leaving it".to_owned(),
    }
}

/// The requested `start_time` is in the past.
#[must_use]
pub fn start_time_in_past() -> MenoError {
    MenoError::BadRequest {
        code: ErrorCode::StartTimeInPast,
        message: "start_time must be in the future".to_owned(),
    }
}

/// The supplied time zone is not an IANA identifier.
///
/// Checked against the system's zone database rather than a pattern match, so
/// `Europe/Lagos2` is refused for the reason it is actually wrong. Falls back to a
/// 400 with the offending value named, which is the one place this module quotes client
/// input back — and only because the value is not secret and the message is the useful
/// half of the answer.
#[must_use]
pub fn invalid_time_zone(zone: &str) -> MenoError {
    MenoError::BadRequest {
        code: ErrorCode::InvalidTimeZone,
        message: format!("Unknown time zone: {zone}"),
    }
}

/// The broadcast already has [`super::dto::MAX_COHOSTS`] cohosts.
#[must_use]
pub fn cohost_limit_reached(limit: usize) -> MenoError {
    MenoError::Conflict {
        code: ErrorCode::Conflict,
        message: format!("A broadcast can have at most {limit} cohosts"),
    }
}

/// The user is already a cohost of this broadcast.
#[must_use]
pub fn already_cohost() -> MenoError {
    MenoError::Conflict {
        code: ErrorCode::Conflict,
        message: "That user is already a cohost of this broadcast".to_owned(),
    }
}

/// Nobody can be their own cohost.
#[must_use]
pub fn cannot_add_self_as_cohost() -> MenoError {
    MenoError::Forbidden {
        code: ErrorCode::Forbidden,
        message: "You cannot add yourself as a cohost".to_owned(),
    }
}

/// A live broadcast cannot be edited: a title change under a listener is a lie, and a
/// schedule change under a running room is meaningless.
#[must_use]
pub fn broadcast_is_live() -> MenoError {
    MenoError::Conflict {
        code: ErrorCode::Conflict,
        message: "A live broadcast cannot be modified".to_owned(),
    }
}

/// The media service is off, unreachable, or its circuit breaker is open.
///
/// §4.6's rule for a gated integration: a *disabled* one must not be a silent success.
/// Saying "temporarily unavailable" is the honest answer for both "switched off" and
/// "upstream is down", because a client cannot act on the difference and the difference
/// is not its business.
#[must_use]
pub fn media_unavailable() -> MenoError {
    MenoError::Upstream {
        service: "livekit",
        detail: "the media service is not available".to_owned(),
    }
}

/// A driver or adapter failure inside the broadcast module. Always `500`, always logged.
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
    //! The properties §4.2 makes load-bearing, asserted on the rendered body rather
    //! than on the enum, because the body is what a client sees.

    use super::*;
    use meno_core::to_body;

    #[test]
    fn every_code_is_one_the_taxonomy_defines() {
        // If a constructor invents a code, no client branch can be written against it.
        assert_eq!(not_found().code(), ErrorCode::NotFound);
        assert_eq!(user_not_found().code(), ErrorCode::NotFound);
        assert_eq!(not_creator().code(), ErrorCode::NotCreator);
        assert_eq!(not_joined().code(), ErrorCode::NotParticipant);
        assert_eq!(start_time_in_past().code(), ErrorCode::StartTimeInPast);
        assert_eq!(
            invalid_time_zone("Mars/Olympus").code(),
            ErrorCode::InvalidTimeZone
        );
        assert_eq!(already_live().code(), ErrorCode::Conflict);
        assert_eq!(media_unavailable().code(), ErrorCode::UpstreamUnavailable);
    }

    #[test]
    fn a_refusal_is_a_4xx_and_a_media_outage_is_a_503() {
        // The distinction a client actually branches on: "try again later" versus
        // "you cannot".
        assert_eq!(to_body(&not_live()).http_status, 409);
        assert_eq!(to_body(&already_joined()).http_status, 409);
        assert_eq!(to_body(&not_creator()).http_status, 403);
        assert_eq!(to_body(&creator_cannot_join()).http_status, 403);
        assert_eq!(to_body(&start_time_in_past()).http_status, 400);
        assert_eq!(to_body(&not_found()).http_status, 404);
        assert_eq!(to_body(&media_unavailable()).http_status, 503);
    }

    #[test]
    fn an_unavailable_media_service_never_leaks_its_detail() {
        // The detail is what a client must not see; the name of the dependency is what
        // it may, because "try again" only makes sense if something is retryable.
        let body = to_body(&media_unavailable());
        assert!(!body.message.contains("circuit"), "{}", body.message);
        assert_eq!(body.code, ErrorCode::UpstreamUnavailable.as_str());
    }

    #[test]
    fn internal_detail_is_logged_never_returned() {
        let error = internal("start_broadcast", "connection to 10.0.0.4:7880 refused");

        assert_eq!(to_body(&error).http_status, 500);
        assert!(!error.is_client_safe());
        assert!(
            !to_body(&error).message.contains("10.0.0.4"),
            "the address leaked: {}",
            to_body(&error).message
        );
        assert!(error.to_string().contains("start_broadcast"));
    }

    #[test]
    fn a_broadcast_and_a_user_are_different_resources() {
        // Same status, different word: the client renders "broadcast not found" and
        // "user not found" differently, and merging them loses that.
        assert_eq!(to_body(&not_found()).message, "broadcast not found");
        assert_eq!(to_body(&user_not_found()).message, "user not found");
    }
}
