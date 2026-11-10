//! Notifications' contribution to the single error taxonomy (plan §4.2).
//!
//! No `NotificationError` enum: the port replaces `master`'s variant set — which
//! carried `CursorError`, `sqlx::Error`, `fred::error::Error` and `anyhow::Error`
//! straight into the wire type — with named constructors over [`MenoError`]. The
//! cursor failure is already the shared `INVALID_CURSOR`; a driver failure is a
//! logged `500` that carries no schema text.
//!
//! # The one refusal this module has
//!
//! A notification belongs to exactly one recipient, and every write is scoped by
//! `owner_id` in the statement. Marking somebody else's notification read therefore
//! changes nothing, and the answer is the same `404` a nonexistent id gets — the
//! caller's instinct should not be "wrong account", it should be "not there".
//! Deleting is the exception: `DELETE` is idempotent in every other module here, so
//! deleting what is not there is a success.
//!
//! # A missing template is a 500, not a 4xx
//!
//! `master` logged and answered `INTERNAL_ERROR` for `TemplateNotFound`, and this
//! keeps that: the type code comes from a call site in this codebase, never from a
//! request, so a missing template is a deployment that did not run migration `0012`'s
//! seed — an operator's problem, not the caller's.

use meno_core::{Error as MenoError, ErrorCode};

/// No live notification carries this id for this owner.
#[must_use]
pub fn notification_not_found() -> MenoError {
    MenoError::NotFound {
        resource: "notification",
        code: ErrorCode::NotFound,
    }
}

/// No active template is registered for this type code.
///
/// A `500`: see the module note. The detail names the code so the log line is
/// actionable, and `to_body` replaces it before it reaches the client.
#[must_use]
pub fn template_missing(type_code: &str) -> MenoError {
    MenoError::Internal {
        context: "notification_template",
        detail: format!("no active template for notification type '{type_code}'"),
    }
}

/// A driver failure inside the notifications module. Always `500`, always logged.
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
        assert_eq!(notification_not_found().code(), ErrorCode::NotFound);
        assert_eq!(
            template_missing("user_subscribed").code(),
            ErrorCode::Internal
        );
        assert_eq!(internal("load_page", "boom").code(), ErrorCode::Internal);
    }

    #[test]
    fn a_refusal_is_a_4xx_and_a_missing_template_is_a_5xx() {
        assert_eq!(to_body(&notification_not_found()).http_status, 404);
        assert_eq!(to_body(&template_missing("nope")).http_status, 500);
    }

    #[test]
    fn a_missing_template_names_the_resource_it_was_looking_for() {
        assert_eq!(
            to_body(&notification_not_found()).message,
            "notification not found"
        );
    }

    #[test]
    fn internal_detail_is_logged_never_returned() {
        let error = internal(
            "load_page",
            "relation \"notification_templates\" does not exist at 10.0.0.4",
        );

        assert_eq!(to_body(&error).http_status, 500);
        assert!(!error.is_client_safe());
        assert!(
            !to_body(&error).message.contains("10.0.0.4"),
            "the address leaked: {}",
            to_body(&error).message
        );
        assert!(error.to_string().contains("load_page"));
    }

    #[test]
    fn a_missing_template_logs_the_code_but_does_not_send_it() {
        let error = template_missing("live_broadcast_started");

        assert!(error.to_string().contains("live_broadcast_started"));
        assert!(
            !to_body(&error).message.contains("live_broadcast_started"),
            "an internal template name is not the client's business"
        );
    }
}
