//! Subscribers' contribution to the single error taxonomy (plan §4.2).
//!
//! No `SubscribersError` enum: three named constructors over [`MenoError`], the same shape
//! as [`crate::modules::notes::error`], [`crate::modules::chat::error`] and
//! [`crate::modules::broadcast::error`]. `master`'s enum had seven variants, two of which
//! no code path could produce (`SubscriptionNotFound`, and `Cursor` had already become
//! `From<CursorError> for Error` in `crates/core`).
//!
//! # The four refusals, and why there are only two of them
//!
//! | Condition | Answer |
//! | --- | --- |
//! | The caller tries to follow itself | `400 BAD_REQUEST` |
//! | The user being followed, unfollowed or listed does not exist | `404 NOT_FOUND` |
//! | The caller is not following somebody they are unfollowing | `200`, idempotently |
//! | A driver failure | `500 INTERNAL_ERROR`, detail logged and never returned |
//!
//! Three separate conditions collapse into one 404 because the taxonomy keeps one
//! `NOT_FOUND` code with the resource in the message; the client learns *which* thing was
//! missing from `"user not found"` and its own request. `master` answered 400 for the
//! self-follow case, which is kept: the request is not forbidden — it is impossible, and
//! the database's `user_subscribers_no_self_follow` constraint says so too.

use meno_core::{Error as MenoError, ErrorCode};

/// No live account carries this id, or it has been soft-deleted.
///
/// One constructor for all three places the id can arrive — the user being followed, the
/// user being unfollowed, and the owner of a list — because they are the same fact about
/// the same table.
#[must_use]
pub fn user_not_found() -> MenoError {
    MenoError::NotFound {
        resource: "user",
        code: ErrorCode::NotFound,
    }
}

/// The caller tried to follow itself.
#[must_use]
pub fn cannot_subscribe_to_self() -> MenoError {
    MenoError::BadRequest {
        code: ErrorCode::BadRequest,
        message: "You cannot subscribe to yourself".to_owned(),
    }
}

/// A driver failure inside the subscribers module. Always `500`, always logged.
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
        assert_eq!(user_not_found().code(), ErrorCode::NotFound);
        assert_eq!(cannot_subscribe_to_self().code(), ErrorCode::BadRequest);
    }

    #[test]
    fn a_refusal_is_a_4xx() {
        assert_eq!(to_body(&user_not_found()).http_status, 404);
        assert_eq!(to_body(&cannot_subscribe_to_self()).http_status, 400);
    }

    #[test]
    fn a_missing_user_names_the_resource_that_was_missing() {
        // The taxonomy has one `NOT_FOUND` code, so the message is the only place a client
        // can learn *what* was absent.
        assert_eq!(to_body(&user_not_found()).message, "user not found");
    }

    #[test]
    fn internal_detail_is_logged_never_returned() {
        let error = internal(
            "create_follow",
            "relation \"user_subscribers\" does not exist",
        );

        assert_eq!(to_body(&error).http_status, 500);
        assert!(!error.is_client_safe());
        assert!(
            !to_body(&error)
                .message
                .contains("user_subscribers\" does not exist"),
            "the driver text leaked: {}",
            to_body(&error).message
        );
        assert!(error.to_string().contains("create_follow"));
    }
}
