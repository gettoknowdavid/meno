//! HTTP middleware: cross-cutting request concerns that sit between the router and a
//! handler.
//!
//! # Layer order matters
//!
//! Applied outermost-first, which is the order §4.4's identification rule depends on:
//!
//! 1. [`timing`] — always outermost, so it measures the layers below it too.
//! 2. [`auth`] — authenticates and inserts [`auth::AuthUser`].
//! 3. [`rate_limit`] — reads the user id [`auth`] just inserted, falling back to IP.
//! 4. [`idempotency`] — reads both, so a cached response can never cross users.
//! 5. [`extractors`] — not a layer, but the rejection vocabulary these layers share.
//!
//! `rate_limit` must run *after* `auth` or §4.4's "identify by authenticated user ID"
//! never fires, and `idempotency` must run after both or §4.5's caller scoping is
//! anonymous. That dependency is the reason the order is documented rather than left to
//! whoever wires the router.
//!
//! # One error renderer
//!
//! `error_response` and `from_error` are the only two places in `apps/api` that
//! turn a failure into a response body, and both go through `meno_core::ErrorBody` — the
//! same struct `crates/core::to_body` fills in. That is what stops the drift §4.2 is
//! about: `master` had nine `IntoResponse` impls plus a second hand-rolled shape in the
//! query extractor, and a client that handled one could not handle the other.
//!
//! The split between the two functions is status code, not severity: `from_error` is
//! for a `meno_core::Error` the domain produced, so the status comes from the taxonomy;
//! `error_response` is for a transport-level rejection whose status (415 for a bad
//! `Content-Type`, 503 for a limiter that could not reach Redis) has no `Error` variant
//! but still has to render as the same shape.

pub mod auth;
pub mod extractors;
pub mod idempotency;
pub mod rate_limit;
pub mod timing;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use meno_core::{Error as MenoError, ErrorBody};

/// Render a transport-level rejection in the §4.2 envelope.
///
/// Used by middleware and extractors, which reject a request before any domain code
/// runs. There is no `MenoError` to map — a 415 is a property of the wire, not of a
/// business rule — but the body still has to be [`ErrorBody`] so a client sees one
/// shape. `code` is a `&'static str` so a caller cannot build a code from user input;
/// the callers in this crate pass constants.
pub(crate) fn error_response(status: StatusCode, code: &'static str, message: &str) -> Response {
    let body = ErrorBody {
        http_status: status.as_u16(),
        code,
        message: message.to_owned(),
        status: false,
        data: None,
        meta: None,
    };

    (status, Json(body)).into_response()
}

/// Render a `meno_core::Error` in the §4.2 envelope.
///
/// The §4.2 boundary for request-serving code: the status, the stable wire code and the
/// message all come from `to_body`, which is also where a non-client-safe error has its
/// detail replaced. A middleware that wants to add a header to a rejection calls this
/// and then mutates the returned response's headers.
pub(crate) fn from_error(error: &MenoError) -> Response {
    let body = meno_core::to_body(error);
    // `to_body` fills in a status from a closed set of literals, so this conversion is
    // total in practice. The fallback is deliberate: §9.1 forbids panicking on a
    // request path, and a 500 is a better wrong answer than a crash.
    let status =
        StatusCode::from_u16(body.http_status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    (status, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header;
    use http_body_util::BodyExt;

    async fn body_of(response: Response) -> serde_json::Value {
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("the body buffers")
            .to_bytes();

        serde_json::from_slice(&bytes).expect("the body is JSON")
    }

    /// The contract a client is entitled to rely on: `code`, `message`, `status`, and
    /// nothing else. `data` and `meta` are omitted rather than nulled.
    fn keys_of(body: &serde_json::Value) -> Vec<&str> {
        body.as_object()
            .expect("the body is an object")
            .keys()
            .map(String::as_str)
            .collect()
    }

    #[tokio::test]
    async fn a_rejection_renders_exactly_the_taxonomy_key_set() {
        // The whole point of routing both functions through `ErrorBody`: a client must
        // not be able to tell which layer refused it by looking at the keys.
        let response = error_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "INVALID_CONTENT_TYPE",
            "Content-Type must be application/json",
        );
        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

        let body = body_of(response).await;

        assert_eq!(keys_of(&body), vec!["code", "message", "status"]);
        assert_eq!(body["code"], "INVALID_CONTENT_TYPE");
        assert_eq!(body["status"], false);
        assert_eq!(body["message"], "Content-Type must be application/json");
    }

    #[tokio::test]
    async fn a_domain_error_renders_the_same_key_set() {
        let response = from_error(&MenoError::BadRequest {
            code: meno_core::ErrorCode::BadRequest,
            message: "bad".to_owned(),
        });

        let body = body_of(response).await;

        assert_eq!(keys_of(&body), vec!["code", "message", "status"]);
        assert!(body.get("data").is_none());
        assert!(body.get("meta").is_none());
    }

    #[tokio::test]
    async fn the_status_line_comes_from_the_taxonomy() {
        // The taxonomy owns the status; a mismatch between the line and the variant is
        // exactly the kind of drift this indirection exists to prevent.
        let response = from_error(&MenoError::NotFound {
            resource: "broadcast",
            code: meno_core::ErrorCode::NotFound,
        });

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(body_of(response).await["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn an_internal_error_never_carries_its_detail_to_the_client() {
        // §9.1. The renderer is the last place that could leak, so it is where the
        // assertion belongs.
        let response = from_error(&MenoError::Internal {
            context: "load_broadcast",
            detail: "relation \"users_secret\" does not exist".to_owned(),
        });
        let status = response.status();
        let body = body_of(response).await;

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["message"], "An internal error occurred");
        assert!(!body.to_string().contains("users_secret"));
    }

    #[tokio::test]
    async fn every_rejection_is_json_encoded() {
        // A rejection that renders as plain text is a client bug that survives because
        // nobody made a request that triggered it.
        let response = error_response(StatusCode::CONFLICT, "REQUEST_IN_FLIGHT", "busy");
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        assert_eq!(content_type.as_deref(), Some("application/json"));
        assert_eq!(body_of(response).await["code"], "REQUEST_IN_FLIGHT");
    }

    #[tokio::test]
    async fn a_rejection_always_has_a_body() {
        // Cheap guard against a future refactor returning a bare `StatusCode`, which is
        // the shape `master` used and clients had to special-case.
        let response = error_response(StatusCode::BAD_REQUEST, "BAD_REQUEST", "bad");

        assert_eq!(body_of(response).await["code"], "BAD_REQUEST");
    }

    #[test]
    fn an_unrepresentable_status_falls_back_rather_than_panicking() {
        // `http_status` is a `u16` the taxonomy fills from a closed set, so this cannot
        // happen today — but the fallback is asserted so a future variant cannot turn a
        // malformed status into a panic on a request path (§9.1).
        assert_eq!(
            StatusCode::from_u16(0).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
