//! Moderation's contribution to the single error taxonomy (plan §4.2).
//!
//! No enum: named constructors over [`MenoError`], the same shape as
//! [`crate::modules::chat::error`] and [`crate::modules::subscribers::error`].

use meno_core::{Error as MenoError, ErrorCode};

/// A report names no live account or broadcast.
#[must_use]
pub fn subject_not_found() -> MenoError {
    MenoError::NotFound {
        resource: "report subject",
        code: ErrorCode::NotFound,
    }
}

/// Nobody carries this report id.
#[must_use]
pub fn report_not_found() -> MenoError {
    MenoError::NotFound {
        resource: "report",
        code: ErrorCode::NotFound,
    }
}

/// No live account carries this id, or it has been soft-deleted.
#[must_use]
pub fn user_not_found() -> MenoError {
    MenoError::NotFound {
        resource: "user",
        code: ErrorCode::NotFound,
    }
}

/// A block names the caller's own account.
///
/// A `400`: the request is well formed and the state it asks for is impossible — the
/// migration's `CHECK` makes a self-block unrepresentable, so refusing here is the only
/// place the caller can be told why.
#[must_use]
pub fn cannot_block_self() -> MenoError {
    MenoError::BadRequest {
        code: ErrorCode::BadRequest,
        message: "You cannot block yourself".to_owned(),
    }
}

/// The queue is an operator's, and this caller is not one.
#[must_use]
pub fn not_an_operator() -> MenoError {
    MenoError::Forbidden {
        code: ErrorCode::Forbidden,
        message: "This is an operator-only endpoint".to_owned(),
    }
}

/// The caller is blocked by the account they are trying to reach.
///
/// Separate from [`not_an_operator`] and from the subscriber module's 404 because the
/// caller can act on it: a block is a decision somebody made about them, not a missing
/// row.
#[must_use]
pub fn blocked() -> MenoError {
    MenoError::Forbidden {
        code: ErrorCode::Forbidden,
        message: "This account has blocked you".to_owned(),
    }
}
