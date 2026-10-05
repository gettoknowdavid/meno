//! The layers every request passes through, composed in one place (plan §9.3).
//!
//! # Why they live in `routes/`
//!
//! §9.3 makes the router the place cross-cutting concerns attach: a layer reachable
//! from a module's own router is a layer that can be forgotten when that module is
//! mounted, and the route table is the security surface — so the stack that guards it
//! is reviewable in one file, in order.
//!
//! # The order, outermost first
//!
//! 1. **Set `x-request-id`** — generates the id when the client did not send one. It
//!    is outermost because the layer below copies whatever the request carries, so
//!    the id must exist before `Propagate` inspects it.
//! 2. **Propagate `x-request-id`** — copies the id from the request onto the response,
//!    so the answer carries it even when a layer further in answers with an error.
//! 3. **CORS** — a preflight should be answered without touching the handler stack at
//!    all, and a refusal should still carry the request id from (1).
//! 4. **`TraceLayer`** — creates the request *span*; the timing middleware writes the
//!    per-request line into it.
//! 5. **`timing_middleware`** — measures the handler rather than the layers above it
//!    (§4.4's ordering rule says timing is outermost *of the auth stack*, which lives
//!    below this file).
//! 6. **Metrics** — innermost, so the recorded duration is the handler stack's, and
//!    so a CORS-rejected preflight is not counted as an API request.
//!
//! Applied in code bottom-up: `.layer` wraps what is already there, so the last layer
//! applied is the outermost — the list above is the reverse of the call order. The
//! request-id pair is the order that matters: `Set` *outside* `Propagate` is what lets
//! propagation see the id that was just generated, and axum's `.layer` (unlike
//! `tower::ServiceBuilder`) applies later calls further out — reversing them yields a
//! response with no id at all, which is how the test below found it.

use std::time::Duration;

use axum::Router;
use axum::http::{HeaderName, HeaderValue, Method, header};
use axum::middleware::from_fn;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::middleware::timing::timing_middleware;

/// The header the request id travels on, both directions.
///
/// One spelling, used for generation, propagation, CORS exposure and any future
/// logging field, so a client can always find the id a log line refers to.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// How long a browser may cache one CORS preflight.
///
/// Ten minutes: long enough that a burst of API calls costs one preflight, short
/// enough that a `CORS_ORIGINS` change reaches browsers without a long tail.
const PREFLIGHT_MAX_AGE: Duration = Duration::from_secs(600);

/// Wrap `router` in the full stack, using this deployment's origins.
///
/// Generic over the router's state so it can wrap the table before — or after —
/// `.with_state`; nothing it does cares what the handlers hold.
pub fn apply<S>(router: Router<S>, config: &Config) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let request_id = HeaderName::from_static(REQUEST_ID_HEADER);

    router
        // Innermost first — see the module docs for the resulting outermost-first list.
        .layer(crate::infrastructure::metrics::Metrics::layer())
        .layer(from_fn(timing_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(cors(&config.origins))
        // Propagate applied first, Set second: in axum the *last* layer applied is the
        // outermost, and Set must run before Propagate reads the header back.
        .layer(PropagateRequestIdLayer::new(request_id.clone()))
        .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
}

/// The CORS policy, built from `CORS_ORIGINS` (§4.6).
///
/// Origins are validated once, at startup: `config.rs` refuses to boot on an origin
/// that is not `http(s)://…`, precisely because a silently-dropped entry becomes a
/// CORS failure with no clue as to why.
fn cors(origins: &[String]) -> CorsLayer {
    let layer = CorsLayer::new()
        // The verbs the API exposes. `OPTIONS` is named because a preflight asks for
        // it explicitly rather than because the router serves it.
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            HeaderName::from_static("idempotency-key"),
            HeaderName::from_static(REQUEST_ID_HEADER),
        ])
        // What a cross-origin client is allowed to *read*: the correlation id and the
        // §4.4 rate-limit vocabulary. Without this list a browser hides them from
        // JavaScript even though the server sends them.
        .expose_headers([
            HeaderName::from_static(REQUEST_ID_HEADER),
            HeaderName::from_static(crate::middleware::rate_limit::HEADER_LIMIT),
            HeaderName::from_static(crate::middleware::rate_limit::HEADER_REMAINING),
            HeaderName::from_static(crate::middleware::rate_limit::HEADER_RESET),
            HeaderName::from_static(crate::middleware::rate_limit::HEADER_RETRY_AFTER),
        ])
        .max_age(PREFLIGHT_MAX_AGE);

    // `AllowOrigin::list`, not a loop of `allow_origin(single)`. Two traps, both found
    // by the tests below rather than by reading docs:
    //
    // - a single `HeaderValue` becomes a *constant* that tower-http echoes on every
    //   response regardless of the request's `Origin`;
    // - each `allow_origin` call **overrides** the previous one, so a loop would end
    //   up enforcing only the last entry in `CORS_ORIGINS`.
    //
    // The list form matches the request's `Origin` against every configured value and
    // emits nothing on a miss — which is the refusal `an_unlisted_origin…` asserts.
    // An unusable entry is skipped rather than panicking: §9.1's no-panic rule applies
    // to wiring as well, and `config.rs` has already refused the malformed cases.
    let allowed: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|origin| match HeaderValue::from_bytes(origin.as_bytes()) {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!(origin = %origin, detail = %error, "skipping an unusable CORS origin");
                None
            }
        })
        .collect();

    if allowed.is_empty() {
        // Not fatal — the API still serves same-origin traffic — but an operator must
        // see it, because the symptom would otherwise be every browser call failing
        // CORS with nothing in the logs.
        tracing::warn!("no usable CORS origin; cross-origin requests will be refused");
    }

    layer.allow_origin(AllowOrigin::list(allowed))
}

#[cfg(test)]
mod tests {
    //! The properties a client and an operator rely on: the request id survives both
    //! directions, a listed origin is echoed, an unlisted one is refused, and the
    //! response shape still says `Vary: origin` so caches do not mix the two.

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::*;

    fn config_with(origins: &[&str]) -> Config {
        use crate::config::MapSource;

        // The same shape `config.rs`'s own tests build: only `CORS_ORIGINS` matters to
        // `apply`, but a fixture that skips required fields would stop being a
        // `Config` the moment validation changed.
        Config::from_source(
            &MapSource::new()
                .with("ENV", "dev")
                .with("PORT", "8080")
                .with("DATABASE_URL", "postgres://u:p@localhost:5432/meno")
                .with("REDIS_URL", "redis://localhost:6379")
                .with("JWT_SECRET", "a-real-secret-value")
                .with("JWT_REFRESH_SECRET", "another-real-secret-value")
                .with("CORS_ORIGINS", &origins.join(",")),
        )
        .expect("the fixture configuration is valid")
    }

    async fn stack() -> Router {
        let router = Router::new().route("/hello", axum::routing::get(|| async { "hi" }));
        apply(router, &config_with(&["https://app.example.com"]))
    }

    #[tokio::test]
    async fn a_request_without_an_id_gets_one_and_the_response_carries_it_back() {
        let response = stack()
            .await
            .oneshot(
                Request::builder()
                    .uri("/hello")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("a response");

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers().get(REQUEST_ID_HEADER).is_some(),
            "every response must be correlatable to a log line"
        );
    }

    #[tokio::test]
    async fn a_client_supplied_id_is_preserved_rather_than_replaced() {
        let response = stack()
            .await
            .oneshot(
                Request::builder()
                    .uri("/hello")
                    .header(REQUEST_ID_HEADER, "client-supplied-id")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("a response");

        assert_eq!(
            response
                .headers()
                .get(REQUEST_ID_HEADER)
                .and_then(|v| v.to_str().ok()),
            Some("client-supplied-id"),
            "a proxy that already assigned an id must not have it silently overwritten"
        );
    }

    #[tokio::test]
    async fn a_listed_origin_passes_cors() {
        let response = stack()
            .await
            .oneshot(
                Request::builder()
                    .uri("/hello")
                    .header(header::ORIGIN, "https://app.example.com")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("a response");

        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|v| v.to_str().ok()),
            Some("https://app.example.com"),
            "the allowed origin is echoed exactly, not with a wildcard"
        );
    }

    #[tokio::test]
    async fn an_unlisted_origin_gets_no_cors_header_at_all() {
        // The browser is the enforcement point: withholding the header is the refusal.
        let response = stack()
            .await
            .oneshot(
                Request::builder()
                    .uri("/hello")
                    .header(header::ORIGIN, "https://evil.example.com")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("a response");

        assert!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none(),
            "an origin outside CORS_ORIGINS must not be granted"
        );
    }
}
