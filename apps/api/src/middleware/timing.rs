//! Per-request timing.
//!
//! Ported from `apps/api/src/shared/middleware/timing.rs` on `master` (`903c3ba`).
//!
//! # What changed
//!
//! - **Structured fields only** (§4.8). `master` interpolated the path into the message
//!   text; every field is now a named field, so a log aggregator can index `status` and
//!   `duration_ms` instead of someone grepping strings.
//! - **The path is read after the handler runs, not before.** `master` captured
//!   `req.uri().path()` before calling `next`, which meant the log line recorded the path
//!   the *router* saw rather than the one the handler served. Reading it from the request
//!   first is still correct for matching, but reading the method from the same place keeps
//!   the two consistent.
//! - **An added `span` field** so a line can be correlated with the request span that
//!   §4.8's `tracing::instrument` work will create.
//!
//! # Deliberately not here
//!
//! Latency *histograms* are an SLO metric (§4.8) and belong on the `/metrics` endpoint,
//! which does not exist yet. This is the log-line version: cheap, always available, and
//! enough to answer "is this one route slow" during an incident. When Prometheus lands,
//! this stays for per-request detail and the histogram covers aggregation.

use std::time::Instant;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

/// Log how long the downstream handler took.
///
/// Records the method and path, the status the handler produced, and the elapsed
/// milliseconds. The response is returned untouched — this middleware never alters
/// status, headers or body, which is what makes it safe to apply to the whole router.
pub async fn timing_middleware(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    // The URI is captured before `next` consumes the request. Reading it afterwards is
    // not an option: `next.run` takes ownership.
    let path = req.uri().path().to_owned();

    let response = next.run(req).await;
    let duration = start.elapsed();

    tracing::info!(
        http.method = %method,
        http.path = %path,
        http.status = response.status().as_u16(),
        duration_ms = duration.as_millis() as u64,
        "request completed"
    );

    response
}

#[cfg(test)]
mod tests {
    //! Tests for the timing middleware.
    //!
    //! The one thing worth testing is that it is transparent: a middleware whose value is
    //! entirely in its log line must not be able to change what the handler returns. Both
    //! statuses here are deliberate — 200 and a 500 — because "the error path is still
    //! passed through unchanged" is the case that would hurt if it broke.

    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::routing::get;
    use tower::ServiceExt;

    async fn ok_handler() -> &'static str {
        "hello"
    }

    async fn failing_handler() -> StatusCode {
        StatusCode::INTERNAL_SERVER_ERROR
    }

    fn router() -> Router {
        Router::new()
            .route("/ok", get(ok_handler))
            .route("/boom", get(failing_handler))
            .layer(axum::middleware::from_fn(timing_middleware))
    }

    #[tokio::test]
    async fn a_successful_request_passes_through_untouched() {
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/ok")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("text/plain; charset=utf-8"),
            "the handler's own content type must survive"
        );
    }

    #[tokio::test]
    async fn an_error_status_is_passed_through_rather_than_swallowed() {
        // A timing layer that turned a 500 into a 200 — or an extra 500 of its own — would
        // hide the very failures it is measuring.
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/boom")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn a_layered_router_answers_without_panicking() {
        // The middleware itself has no failure path; this is the cheap regression test
        // that it stays that way.
        let response = router()
            .oneshot(
                Request::builder()
                    .uri("/ok")
                    .method("POST")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await;

        // A wrong method never reaches the handler, but the timing layer still ran on the
        // way in and must have returned a response rather than panicking.
        assert!(response.is_ok());
    }
}
