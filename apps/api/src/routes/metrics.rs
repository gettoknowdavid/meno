//! `GET /metrics`, private (plan §7.5, §4.8).
//!
//! # Why it is gated at all
//!
//! §7.5 records that `master`'s `/metrics` was **unauthenticated and merged before the
//! auth layer** — public exposure of request rates, routes and queue depth, which is
//! reconnaissance for whoever wants to find the quiet endpoint. The plan's three
//! options for fixing it are a private bind, a static token, or a private network; a
//! static token is the one that can be implemented in the application itself, so it is
//! what this file is.
//!
//! # Fail closed, twice
//!
//! - **No `METRICS_TOKEN` configured → 404.** An unset token must disable the
//!   endpoint, not leave it open waiting for someone to notice; that is the same
//!   §4.6 reasoning as an optional integration, pointed the safe way.
//! - **Wrong or absent `Authorization` → 401**, with `WWW-Authenticate: Bearer` so a
//!   scrape misconfiguration is diagnosable from the client side rather than from an
//!   empty dashboard.
//!
//! The comparison is length-independent (see [`constant_time_eq`]): a token checked
//! byte-by-byte with an early return is a textbook timing oracle, and this one is
//! three lines shorter than the excuse to skip it.

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;

use crate::config::Secret;
use crate::middleware::error_response;
use crate::state::MenoState;

/// What presenting credentials to the endpoint amounts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// The token matched; render the payload.
    Render,
    /// No token is configured; the endpoint does not exist here.
    Disabled,
    /// A token was expected and not matched.
    Denied,
}

/// Decide what a request to `/metrics` gets, from its headers and the configured
/// token — separated from the handler so the three branches are asserted without
/// needing a `MenoState` (which needs a pool and a Redis).
fn authorize(headers: &HeaderMap, expected: Option<&Secret>) -> Outcome {
    let Some(expected) = expected else {
        return Outcome::Disabled;
    };

    let Some(presented) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return Outcome::Denied;
    };

    if constant_time_eq(presented.as_bytes(), expected.expose().as_bytes()) {
        Outcome::Render
    } else {
        Outcome::Denied
    }
}

/// Compare two byte strings without an early return on the first mismatching byte.
///
/// The length check leaks only the *length* of the configured token, which is not the
/// secret; the fold then XORs every byte pair and folds to one accumulator, so equal
/// inputs take the same path regardless of where (or whether) they differ.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    a.iter()
        .zip(b)
        .fold(0_u8, |acc, (left, right)| acc | (left ^ right))
        == 0
}

/// `GET /metrics` — the Prometheus exposition, behind the static token.
///
/// The *recording* happens in the layer `routes::layers` attaches, unconditionally:
/// this handler only decides whether the snapshot leaves the process.
pub async fn handler(State(state): State<MenoState>, headers: HeaderMap) -> Response {
    match authorize(&headers, state.config.metrics_token.as_ref()) {
        Outcome::Disabled => error_response(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "metrics are not enabled on this instance",
        ),
        Outcome::Denied => {
            let mut response = error_response(
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "a valid metrics token is required",
            );
            // The scheme, so an operator knows *how* to authenticate rather than
            // guessing between a static token, a bearer JWT and basic auth.
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
            response
        }
        Outcome::Render => {
            let mut response = Response::new(Body::from(state.metrics.render()));
            // The exposition format's own content type: Prometheus accepts it, and a
            // browser told `text/plain` renders the payload instead of downloading it.
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
            );
            response
        }
    }
}

#[cfg(test)]
mod tests {
    //! The gate's three branches, and the comparison behind the middle one.

    use super::*;

    fn headers_with(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            value.parse::<HeaderValue>().expect("a header"),
        );
        headers
    }

    #[test]
    fn no_configured_token_disables_the_endpoint_rather_than_opening_it() {
        // §7.5's failure mode was an *unauthenticated* metrics endpoint. The default
        // must therefore be the closed door, even for a request that presents a
        // perfectly valid-looking token.
        assert_eq!(
            authorize(&headers_with("Bearer anything"), None),
            Outcome::Disabled
        );
        assert_eq!(authorize(&HeaderMap::new(), None), Outcome::Disabled);
    }

    #[test]
    fn a_missing_or_malformed_authorization_header_is_denied() {
        let expected = Secret::new("s3cr3t");

        assert_eq!(
            authorize(&HeaderMap::new(), Some(&expected)),
            Outcome::Denied
        );
        // No `Bearer ` prefix: a raw token is not a bearer credential.
        assert_eq!(
            authorize(&headers_with("s3cr3t"), Some(&expected)),
            Outcome::Denied
        );
        // A non-UTF8 header never reaches the comparison.
        let mut raw = HeaderMap::new();
        raw.insert(
            header::AUTHORIZATION,
            HeaderValue::from_bytes(&[0xff, 0xfe]).expect("an opaque header"),
        );
        assert_eq!(authorize(&raw, Some(&expected)), Outcome::Denied);
    }

    #[test]
    fn the_right_token_renders_and_the_wrong_one_does_not() {
        let expected = Secret::new("s3cr3t");

        assert_eq!(
            authorize(&headers_with("Bearer s3cr3t"), Some(&expected)),
            Outcome::Render
        );
        assert_eq!(
            authorize(&headers_with("Bearer almost"), Some(&expected)),
            Outcome::Denied
        );
        // One character off, same length: the case a `starts_with` check would wave
        // through.
        assert_eq!(
            authorize(&headers_with("Bearer s3cr3sx"), Some(&expected)),
            Outcome::Denied
        );
    }

    #[test]
    fn the_comparison_treats_length_as_a_miss_not_a_panic() {
        assert!(constant_time_eq(b"", b""));
        assert!(constant_time_eq(b"token", b"token"));
        assert!(!constant_time_eq(b"token", b"tok"));
        assert!(!constant_time_eq(b"token", b"tokex"));
        assert!(!constant_time_eq(b"differ", b"diffeq"));
    }
}
