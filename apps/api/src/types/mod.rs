//! Wire types shared across the HTTP surface.
//!
//! # What belongs here
//!
//! A type goes in this module when **more than one route module speaks it** and its shape is
//! part of the client contract. That is a short list by design:
//!
//! - [`meno_response`] — the single success envelope every endpoint returns.
//!
//! # What does not
//!
//! **Domain DTOs do not live here.** The previous version of this file said that `dto.rs`
//! would be copied from `master`'s `shared/types/`, which would have put every request and
//! response struct for every domain into one module — the thing §4.1 exists to undo. Under
//! the plan's module layering, `modules/auth/dto.rs` owns `LoginRequest`, and
//! `modules/notes/dto.rs` owns `CreateNoteRequest`; `types/` holds only what crosses domains.
//!
//! That also settles the casing question: a nested payload inside `data` keeps its own
//! convention, applied per struct (`CursorPage` emits `nextCursor`/`hasNextPage`), while the
//! envelope itself is `camelCase` because both clients are.
//!
//! # The envelope pair
//!
//! The API has exactly two response shapes, and §4.2 requires that a client never have to
//! special-case between them:
//!
//! | | Type | Home |
//! | --- | --- | --- |
//! | success | [`meno_response::MenoResponse`] | here — it implements `IntoResponse` |
//! | failure | [`meno_core::ErrorBody`] | `crates/core` — pure serde, no `axum` |
//!
//! The failure shape lives in `crates/core` because §2.1 forbids that crate from depending on
//! `axum`, and an error body is pure data. The success shape cannot move for the same reason
//! in reverse: it renders itself through `IntoResponse`. The tests below assert the two agree
//! on the keys a client branches on.

pub mod meno_response;

#[cfg(test)]
mod tests {
    //! The contract between the two envelopes.
    //!
    //! These belong here rather than in `meno_response` because they span both halves, and the
    //! failure half does not live in this crate.

    use super::meno_response::{CODE_OK, MenoResponse};
    use meno_core::{Error, ErrorCode, to_body};

    fn keys(value: &serde_json::Value) -> Vec<String> {
        value
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect()
    }

    fn sort(mut keys: Vec<String>) -> Vec<String> {
        keys.sort();
        keys
    }

    #[test]
    fn both_envelopes_carry_the_same_three_required_keys() {
        // §4.2. A client decodes one response type and branches on `status`; if a failure
        // arrived under different names it would need a special case, which is what this
        // assertion exists to prevent.
        let success = serde_json::to_value(MenoResponse::ok("ok", 1)).expect("serialises");
        let failure = serde_json::to_value(to_body(&Error::NotFound {
            resource: "broadcast",
            code: ErrorCode::NotFound,
        }))
        .expect("serialises");

        for required in ["code", "message", "status"] {
            assert!(
                success.get(required).is_some(),
                "the success envelope is missing `{required}`"
            );
            assert!(
                failure.get(required).is_some(),
                "the failure envelope is missing `{required}`"
            );
        }
    }

    #[test]
    fn neither_envelope_repeats_the_http_status() {
        // Both send it in the status line only. A body field would be redundancy, and a
        // client that trusted it could disagree with the line it arrived on.
        let success = serde_json::to_value(MenoResponse::ok("ok", 1)).expect("serialises");
        let failure = serde_json::to_value(to_body(&Error::Internal {
            context: "x",
            detail: "y".to_owned(),
        }))
        .expect("serialises");

        assert!(success.get("httpStatus").is_none());
        assert!(success.get("http_status").is_none());
        assert!(failure.get("httpStatus").is_none());
        assert!(failure.get("http_status").is_none());
    }

    #[test]
    fn the_status_flag_partitions_success_from_failure() {
        // The one field that must mean exactly one thing across both shapes.
        let success = serde_json::to_value(MenoResponse::ok("ok", 1)).expect("serialises");
        let failure = serde_json::to_value(to_body(&Error::Conflict {
            code: ErrorCode::EmailTaken,
            message: "taken".to_owned(),
        }))
        .expect("serialises");

        assert_eq!(success["status"], serde_json::json!(true));
        assert_eq!(failure["status"], serde_json::json!(false));
    }

    #[test]
    fn a_success_without_a_payload_and_a_failure_without_detail_share_a_key_set() {
        // The two "minimal" shapes. A client that has to branch on which fields are present
        // here cannot write one decoder for both.
        let success = serde_json::to_value(MenoResponse::no_content("done")).expect("serialises");
        let failure = serde_json::to_value(to_body(&Error::RateLimited {
            retry_after_secs: 60,
        }))
        .expect("serialises");

        assert_eq!(sort(keys(&success)), sort(keys(&failure)));
    }

    #[test]
    fn a_code_is_never_a_status_line() {
        // The `master` regression §4.2 names: success sent `"200 OK"` here while errors sent
        // `"BAD_REQUEST"`, so one field carried two conventions depending on the outcome.
        let success = serde_json::to_value(MenoResponse::ok("ok", 1)).expect("serialises");
        let failure = serde_json::to_value(to_body(&Error::BadRequest {
            code: ErrorCode::BadRequest,
            message: "bad".to_owned(),
        }))
        .expect("serialises");

        assert_eq!(success["code"], CODE_OK);
        assert_eq!(failure["code"], ErrorCode::BadRequest.as_str());
        for code in [success["code"].clone(), failure["code"].clone()] {
            let text = code.as_str().expect("a string");
            assert!(
                !text.chars().any(|c| c.is_ascii_digit()),
                "`{text}` reads like a status, so it will move when the status does"
            );
        }
    }

    #[test]
    fn optional_fields_are_omitted_rather_than_nulled() {
        // A Flutter client decoding into `T?` cannot tell `null` from absent, and the
        // contract says omitted.
        let success = serde_json::to_value(MenoResponse::no_content("done")).expect("serialises");
        let failure = serde_json::to_value(to_body(&Error::RateLimited {
            retry_after_secs: 60,
        }))
        .expect("serialises");

        assert!(success.get("data").is_none());
        assert!(failure.get("data").is_none());
        assert!(success.get("meta").is_none());
        assert!(failure.get("meta").is_none());
    }
}
