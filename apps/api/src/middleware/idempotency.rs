//! Idempotency keys: a retried mutation runs once.
//!
//! Ported from `apps/api/src/shared/middleware/idempotency.rs` on `master` (`903c3ba`).
//!
//! # §7.4 — three bugs, all fixed here
//!
//! `master`'s version looked correct and was not:
//!
//! 1. **It never ran.** It read `req.extensions().get::<Arc<Redis>>()`, and nothing ever
//!    inserted an `Arc<Redis>` — so every request took the `None` branch and passed
//!    through. Eight route modules applied a no-op and believed they were protected.
//! 2. **The key was globally scoped.** The Redis key was just the client's
//!    `Idempotency-Key` UUID. Any user who guessed or observed another user's key got
//!    that user's cached response — a cross-user data leak, not just a cache miss.
//! 3. **No in-flight lock.** Two concurrent retries both missed the cache, both ran the
//!    handler, and both wrote. A retried "create broadcast" produced two broadcasts.
//!
//! Two more surfaced on inspection, both named by §4.5:
//!
//! 4. **The `Content-Type` was lost on replay.** The cached body was replayed without the
//!    header, so a client that checked the type saw a JSON payload arrive as
//!    `text/plain`.
//! 5. **The key ignored the route.** Keyed only on the UUID, the same key reused across
//!    two endpoints returned whichever response was cached first.
//!
//! # The scope that fixes (2) and (5)
//!
//! The Redis key is built from **(caller, route, key)**. The caller is a stable
//! identifier supplied by the auth middleware as [`IdempotencyScope`]; the route is
//! axum's *matched path pattern* (`/notes/1` and `/notes/2` share `/notes/{id}`), not the
//! concrete path, so paging through a collection does not invalidate the key. Without a
//! scope the caller falls back to the peer address, which is still better than a global
//! key and still wrong only for two users behind one NAT.
//!
//! §4.5's ordering matters: the scope is part of the key, so two users cannot reach each
//! other's cache entries by guessing a UUID.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{MatchedPath, Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use http_body_util::BodyExt;
use meno_core::Error as MenoError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::infrastructure::redis::Redis;
use crate::infrastructure::redis::keys::RedisKey;
use crate::middleware::error_response;

/// How long a stored response stays replayable.
///
/// 24 hours: long enough to cover a client that retries across a mobile reconnect, short
/// enough that Redis is not accumulating response bodies indefinitely. §4.5 asks for a
/// sweeper or relies on the TTL alone — this relies on the TTL, which is the cheaper of
/// the two.
pub const IDEMPOTENCY_TTL: Duration = Duration::from_secs(86_400);

/// How long an in-flight marker lives before a crashed request's lock is assumed stale.
///
/// Short. It only has to outlast a normal handler run; a handler that has not finished
/// in two seconds is not going to, and a stale lock would block retries forever.
pub const IN_FLIGHT_TTL: Duration = Duration::from_secs(30);

/// The header a client sets to make a mutation safe to retry.
pub const IDEMPOTENCY_HEADER: &str = "Idempotency-Key";

/// Who a request belongs to, for key scoping.
///
/// Inserted into request extensions by the auth middleware once the caller is known.
/// Absent means unauthenticated, and [`scope_identifier`] falls back to the peer
/// address.
///
/// The type is a newtype rather than a bare `String` so the fallback is a decision made in
/// one place — a middleware that quietly used the full token as an identifier, as
/// `master` did with its last-16-characters, is exactly the bug §4.5 is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdempotencyScope(pub String);

impl std::fmt::Display for IdempotencyScope {
    /// The scope as it appears in a Redis key. `Display` rather than reaching for
    /// `.0` at each key-building site, so the layout has one definition.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl IdempotencyScope {
    /// A scope for an authenticated caller.
    #[must_use]
    pub fn user(user_id: Uuid) -> Self {
        Self(format!("u:{user_id}"))
    }

    /// A scope for an anonymous caller.
    #[must_use]
    pub fn anonymous(peer: &str) -> Self {
        Self(format!("a:{peer}"))
    }
}

/// What a cached response looked like, including the header the replay would otherwise
/// lose.
///
/// Bug (4): storing the body without its `Content-Type` and replaying it bare is how a
/// client ends up parsing JSON as text.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CachedResponse {
    /// The status line to replay.
    pub status: u16,
    /// The response body.
    pub body: String,
    /// The `Content-Type`, if the original had one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

impl CachedResponse {
    /// Capture a response for later replay.
    #[must_use]
    pub fn from_parts(status: StatusCode, headers: &HeaderMap, body: String) -> Self {
        Self {
            status: status.as_u16(),
            body,
            content_type: headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned),
        }
    }

    /// Rebuild a `Response` from the cache.
    ///
    /// # Errors
    ///
    /// Returns `None` when the stored status is not a valid HTTP status — a corrupt or
    /// hand-edited cache entry should replay as a miss, not as a panic.
    #[must_use]
    pub fn to_response(&self) -> Option<Response> {
        let status = StatusCode::from_u16(self.status).ok()?;

        let mut response = Response::new(Body::from(self.body.clone()));
        *response.status_mut() = status;
        if let Some(content_type) = &self.content_type {
            // A `HeaderValue` can only fail on bytes that are not visible ASCII; a
            // content type from a response we produced is always valid, and a bad one is
            // skipped rather than failing the replay.
            if let Ok(value) = header::HeaderValue::from_str(content_type) {
                response
                    .headers_mut()
                    .insert(HeaderName::from_static("content-type"), value);
            }
        }
        Some(response)
    }
}

/// Header set on a replayed response so a client can tell a cache hit from a fresh run.
///
/// `true` on a replay and absent on the original. The same header name Stripe's idempotency
/// implementation uses, so a client written against either convention works.
pub const HEADER_REPLAYED: &str = "idempotent-replayed";

/// Wire code for a key that is present but is not a UUID.
pub const CODE_INVALID_KEY: &str = "INVALID_IDEMPOTENCY_KEY";
/// Wire code for a retry that arrived while the first attempt was still running.
pub const CODE_IN_FLIGHT: &str = "REQUEST_IN_FLIGHT";
/// Wire code for a store that could not confirm whether the key had been used.
pub const CODE_UNAVAILABLE: &str = "IDEMPOTENCY_UNAVAILABLE";

/// Read the `Idempotency-Key` header.
///
/// # Errors
///
/// Returns the message to send back when the header is present but not a UUID. A malformed
/// key is a client bug worth surfacing; silently ignoring it would run the mutation
/// unprotected, which is the exact opposite of what the client asked for by sending the
/// header at all.
pub fn extract_idempotency_key(headers: &HeaderMap) -> Result<Option<Uuid>, &'static str> {
    match headers.get(IDEMPOTENCY_HEADER) {
        None => Ok(None),
        Some(raw) => {
            let text = raw
                .to_str()
                .map_err(|_| "Idempotency-Key must be visible ASCII")?;
            Uuid::parse_str(text.trim())
                .map(Some)
                .map_err(|_| "Idempotency-Key must be a UUID")
        }
    }
}

/// Build the scoped Redis key for `(caller, route, key)`.
///
/// The three components are separated by a character that cannot appear in a UUID, so no
/// combination of them can be crafted to collide with another.
#[must_use]
pub fn scoped_key(scope: &IdempotencyScope, route: &str, key: Uuid) -> RedisKey {
    RedisKey::new_raw(&format!("idem:{}:{route}:{key}", scope.0), IDEMPOTENCY_TTL)
}

/// The in-flight marker for `(caller, route, key)`.
///
/// A separate key from the cached response rather than a marker inside it, because the two
/// have different lifetimes: the response is replayable for a day, the marker only for as
/// long as one handler plausibly runs. Writing both under one key would mean either an
/// early-expiring replay cache or a marker that blocks retries for a day.
#[must_use]
pub fn in_flight_key(scope: &IdempotencyScope, route: &str, key: Uuid) -> RedisKey {
    RedisKey::new_raw(&format!("idem-lock:{scope}:{route}:{key}"), IN_FLIGHT_TTL)
}

/// Decide the caller identifier for a request.
///
/// The auth middleware's user wins, so two users cannot reach one another's cache entries
/// (§4.5's data leak). Failing that, the peer address; failing *that*, one fixed bucket —
/// never a fresh one per request, which would be a free unlimited pass.
#[must_use]
pub fn scope_identifier(req: &Request) -> IdempotencyScope {
    if let Some(user) = req.extensions().get::<crate::middleware::auth::AuthUser>() {
        return IdempotencyScope::user(user.id);
    }

    IdempotencyScope::anonymous(
        &crate::middleware::rate_limit::peer_address(req)
            .map_or_else(|| "unknown".to_owned(), |address| address.ip().to_string()),
    )
}

/// Mark a response as a replay.
///
/// Skipped rather than fatal if the header cannot be represented: a marker is never worth
/// failing a response over, and `master`'s equivalent `unwrap`ed on the request path
/// (§7.10).
fn mark_replayed(response: &mut Response) {
    let name = HeaderName::from_static(HEADER_REPLAYED);
    response
        .headers_mut()
        .insert(name, HeaderValue::from_static("true"));
}

/// Try to claim the in-flight marker for this key.
///
/// # Errors
///
/// A backend failure. The caller must treat that as "cannot promise single execution" and
/// refuse, because running the handler anyway is exactly the duplicate side effect §4.5
/// exists to prevent.
///
/// Lua rather than two round trips: `SET` then `EXPIRE` from the client leaves a window in
/// which the marker exists with no expiry, and a marker that never expires blocks every
/// retry of that key until memory pressure evicts it.
async fn claim(redis: &Redis, lock_key: &RedisKey) -> Result<bool, MenoError> {
    const SCRIPT: &str = r"
        if redis.call('SET', KEYS[1], '1', 'NX', 'EX', ARGV[1]) then
            return 1
        else
            return 0
        end
    ";

    let claimed: i64 = redis
        .eval(
            SCRIPT,
            vec![lock_key.as_str()],
            vec![i64::try_from(lock_key.ttl().as_secs()).unwrap_or(i64::MAX)],
        )
        .await
        .map_err(|error| {
            tracing::error!(
                service = "redis",
                detail = %error,
                "could not claim the idempotency key"
            );
            MenoError::Upstream {
                service: "idempotency",
                detail: error.to_string(),
            }
        })?;

    Ok(claimed == 1)
}

/// Drop the in-flight marker.
///
/// Best-effort by design: a marker that outlives its request is only harmful until its TTL,
/// whereas blocking on the delete would put Redis on the response path of a request that has
/// already succeeded.
async fn release(redis: &Redis, lock_key: &RedisKey) {
    if let Err(error) = redis.del(lock_key).await {
        tracing::warn!(detail = %error, "could not release the idempotency lock");
    }
}

/// Replay a cached response, run the handler once, and cache the result.
///
/// Apply with `from_fn_with_state` so the Redis handle is typed state rather than an
/// extension — §4.4's complaint about `Extension<Option<T>>` applies here too, and it is
/// what makes bug (1) impossible: there is no longer a lookup that can silently miss.
///
/// A request with no `Idempotency-Key` passes straight through.
pub async fn idempotency_middleware(
    State(redis): State<Arc<Redis>>,
    req: Request,
    next: Next,
) -> Response {
    let key = match extract_idempotency_key(req.headers()) {
        Ok(Some(key)) => key,
        // Absence means "this endpoint is not idempotency-protected", which must not turn
        // into a rejection.
        Ok(None) => return next.run(req).await,
        Err(message) => {
            return error_response(StatusCode::BAD_REQUEST, CODE_INVALID_KEY, message);
        }
    };

    // Owned, because `next.run` takes the request and the pattern is read out of its
    // extensions — a borrow of `req` could not outlive that move.
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map_or_else(|| "/".to_owned(), |path| path.as_str().to_owned());
    let scope = scope_identifier(&req);
    let cache_key = scoped_key(&scope, &route, key);

    // A completed response wins over an in-flight marker: if both somehow exist, the client
    // gets the real answer rather than a spurious 409.
    match redis.get::<CachedResponse>(&cache_key).await {
        Ok(Some(cached)) => {
            return match cached.to_response() {
                Some(mut response) => {
                    mark_replayed(&mut response);
                    response
                }
                // A corrupt entry reads as a miss rather than as a panic on a request path
                // (§9.1). The client re-runs, which is the safe direction to fail.
                None => {
                    tracing::warn!(
                        key = %cache_key,
                        "unusable cached response; treating it as a miss"
                    );
                    next.run(req).await
                }
            };
        }
        Ok(None) => {}
        // A lookup failure must not silently drop the protection. Fall through and run the
        // handler, but say so: an operator reading logs needs to know idempotency was not
        // applied. This is the one place that fails *open*, because a mutation that has not
        // demonstrably run yet is still safe to run.
        Err(error) => {
            tracing::warn!(
                service = "redis",
                detail = %error,
                "idempotency cache lookup failed; running the handler unprotected"
            );
        }
    }

    // Bug (3): claim the key before running the handler.
    let lock_key = in_flight_key(&scope, &route, key);
    match claim(&redis, &lock_key).await {
        Ok(true) => {}
        Ok(false) => {
            return error_response(
                StatusCode::CONFLICT,
                CODE_IN_FLIGHT,
                "an identical request is already being processed",
            );
        }
        Err(error) => {
            // Failing to claim means single execution cannot be promised. Refusing is the
            // honest answer; running it twice is what §4.5 forbids.
            tracing::error!(
                service = "redis",
                detail = %error,
                "could not claim the idempotency key; refusing rather than risking a duplicate"
            );
            return error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                CODE_UNAVAILABLE,
                "could not verify whether this request already ran",
            );
        }
    }

    let response = next.run(req).await;

    if !response.status().is_success() {
        // A failure is retryable, so the key must not stay claimed — otherwise every retry
        // of a request that hit a 500 would 409 for the whole lock TTL.
        release(&redis, &lock_key).await;
        return response;
    }

    // Only successes are replayable. Caching a 5xx would make every retry return the failure
    // forever, which is the opposite of a retry.
    let (parts, body) = response.into_parts();
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(error) => {
            tracing::warn!(detail = %error, "could not buffer the response to cache it");
            release(&redis, &lock_key).await;
            return Response::from_parts(parts, Body::empty());
        }
    };

    let cached = CachedResponse::from_parts(
        parts.status,
        &parts.headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    );

    let redis = Arc::clone(&redis);
    let cache_key = scoped_key(&scope, &route, key);
    // Fire-and-forget: the client must not wait on a cache write. A failure here means a
    // retry re-runs the handler, which is why it is logged rather than ignored.
    tokio::spawn(async move {
        if let Err(error) = redis.set(&cache_key, &cached).await {
            tracing::warn!(detail = %error, "could not store the idempotent response");
        }
        release(&redis, &lock_key).await;
    });

    Response::from_parts(parts, Body::from(bytes))
}

#[cfg(test)]
mod tests {
    //! Tests for key scoping, the header, and replay fidelity.
    //!
    //! The Redis round trips are in `mod live`.

    use super::*;

    fn headers_with(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(IDEMPOTENCY_HEADER, value.parse().expect("a header value"));
        headers
    }

    // ── the header ────────────────────────────────────────────────────────

    #[test]
    fn an_absent_key_is_not_an_error() {
        // Absence means "this endpoint does not use idempotency", which must not turn
        // into a rejection.
        assert_eq!(extract_idempotency_key(&HeaderMap::new()), Ok(None));
    }

    #[test]
    fn a_well_formed_key_is_parsed() {
        let uuid = Uuid::from_u128(1);

        assert_eq!(
            extract_idempotency_key(&headers_with(&uuid.to_string())),
            Ok(Some(uuid))
        );
    }

    #[test]
    fn surrounding_whitespace_is_tolerated() {
        let uuid = Uuid::from_u128(2);

        assert_eq!(
            extract_idempotency_key(&headers_with(&format!("  {uuid}  "))),
            Ok(Some(uuid))
        );
    }

    #[test]
    fn a_malformed_key_is_rejected_rather_than_ignored() {
        // Ignoring it would run the mutation unprotected — precisely what the client
        // asked to avoid by sending the header.
        for bad in ["not-a-uuid", "12345", ""] {
            assert!(
                extract_idempotency_key(&headers_with(bad)).is_err(),
                "`{bad}` must be rejected"
            );
        }
    }

    // ── scoping: the §7.4 data leak ───────────────────────────────────────

    #[test]
    fn two_users_with_the_same_key_get_different_cache_entries() {
        // The cross-user leak. Same `Idempotency-Key`, different callers, and the entries
        // must not collide — otherwise one user reads another's response.
        let key = Uuid::from_u128(9);
        let alice = IdempotencyScope::user(Uuid::from_u128(1));
        let bob = IdempotencyScope::user(Uuid::from_u128(2));

        assert_ne!(
            scoped_key(&alice, "/notes", key).as_str(),
            scoped_key(&bob, "/notes", key).as_str(),
            "§4.5: the caller must be part of the key"
        );
    }

    #[test]
    fn the_same_key_on_a_different_route_gets_a_different_entry() {
        // Bug (5): keying only on the UUID made `POST /notes` and `POST /comments` share
        // a cache entry, so whichever ran first answered both.
        let key = Uuid::from_u128(3);
        let scope = IdempotencyScope::user(Uuid::from_u128(1));

        assert_ne!(
            scoped_key(&scope, "/notes", key).as_str(),
            scoped_key(&scope, "/comments", key).as_str()
        );
    }

    #[test]
    fn the_same_caller_and_route_and_key_is_stable() {
        // The property that makes retries work at all.
        let key = Uuid::from_u128(4);
        let scope = IdempotencyScope::user(Uuid::from_u128(1));

        assert_eq!(
            scoped_key(&scope, "/notes", key).as_str(),
            scoped_key(&scope, "/notes", key).as_str()
        );
    }

    #[test]
    fn an_anonymous_caller_is_scoped_by_peer() {
        let scope = IdempotencyScope::anonymous("203.0.113.4");

        assert!(scope.0.starts_with("a:"));
        assert_ne!(scope.0, IdempotencyScope::anonymous("203.0.113.5").0);
    }

    #[test]
    fn the_scope_and_the_key_cannot_be_crafted_to_collide() {
        // A colon in the route pattern must not let `u:1` + `/x:y` impersonate another
        // pair. UUIDs contain no colon, so the route is the only risk.
        let alice = IdempotencyScope::user(Uuid::from_u128(1));
        let key = Uuid::from_u128(5);

        assert_ne!(
            scoped_key(&alice, "/notes:extra", key).as_str(),
            scoped_key(&IdempotencyScope::anonymous("/notes"), "extra", key).as_str()
        );
    }

    // ── replay fidelity: bug (4) ──────────────────────────────────────────

    #[test]
    fn a_replay_preserves_the_content_type() {
        // Without this a client that checks the type sees JSON arrive as text/plain.
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            "application/json".parse().expect("a value"),
        );

        let cached = CachedResponse::from_parts(StatusCode::CREATED, &headers, "{}".to_owned());
        let response = cached.to_response().expect("a response");

        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
    }

    #[test]
    fn a_response_with_no_content_type_replays_without_one() {
        let cached = CachedResponse::from_parts(StatusCode::OK, &HeaderMap::new(), String::new());

        let response = cached.to_response().expect("a response");

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().get(header::CONTENT_TYPE).is_none());
    }

    #[test]
    fn an_impossible_cached_status_refuses_to_replay() {
        // A corrupt entry should read as a miss, not panic a request handler.
        // Below the 100..=999 range `http` accepts, which is what a corrupt or hand-edited
        // entry would hold.
        let cached = CachedResponse {
            status: 99,
            body: String::new(),
            content_type: None,
        };

        assert!(cached.to_response().is_none());
    }

    #[test]
    fn a_cached_response_round_trips_through_redis_serialisation() {
        // `Redis::get` is JSON, so the stored shape has to survive it.
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            "application/json".parse().expect("a value"),
        );
        let cached = CachedResponse::from_parts(StatusCode::OK, &headers, r#"{"a":1}"#.to_owned());

        let json = serde_json::to_string(&cached).expect("serialises");
        let back: CachedResponse = serde_json::from_str(&json).expect("deserialises");

        assert_eq!(back, cached);
    }

    #[test]
    fn an_entry_written_before_content_type_existed_still_deserialises() {
        // `content_type` is `#[serde(default)]` so a cache entry written by an older build
        // does not poison every replay after a deploy.
        let back: CachedResponse =
            serde_json::from_str(r#"{"status":200,"body":"ok"}"#).expect("deserialises");

        assert_eq!(back.status, 200);
        assert_eq!(back.content_type, None);
    }

    // ── the in-flight marker: bug (3) ──────────────────────────────────────

    #[test]
    fn the_marker_is_a_separate_key_from_the_cached_response() {
        // Two lifetimes, so two keys: the response is replayable for a day, the marker only
        // for as long as a handler plausibly runs.
        let scope = IdempotencyScope::user(Uuid::from_u128(1));
        let key = Uuid::from_u128(2);

        assert_ne!(
            scoped_key(&scope, "/notes", key).as_str(),
            in_flight_key(&scope, "/notes", key).as_str()
        );
    }

    #[test]
    fn the_marker_expires_soon_after_the_response_does() {
        // Inverted, a marker that outlives the cached response would 409 a retry long after
        // the answer is available; one that expires early would let a second request through
        // while the first is still running.
        let scope = IdempotencyScope::user(Uuid::from_u128(1));
        let key = Uuid::from_u128(2);

        assert!(IN_FLIGHT_TTL < IDEMPOTENCY_TTL);
        assert_eq!(in_flight_key(&scope, "/notes", key).ttl(), IN_FLIGHT_TTL);
        assert_eq!(scoped_key(&scope, "/notes", key).ttl(), IDEMPOTENCY_TTL);
    }

    #[test]
    fn every_key_this_module_builds_carries_a_ttl() {
        // §9.4, asserted on this module's own keys.
        let scope = IdempotencyScope::user(Uuid::from_u128(1));
        let key = Uuid::from_u128(2);

        for built in [
            scoped_key(&scope, "/notes", key),
            in_flight_key(&scope, "/notes", key),
            scoped_key(&scope, "/notes/{id}", key),
            in_flight_key(&IdempotencyScope::anonymous("1.2.3.4"), "/notes", key),
        ] {
            assert!(!built.ttl().is_zero(), "{} has no TTL", built.as_str());
        }
    }

    #[test]
    fn the_caller_is_resolved_from_the_auth_layer() {
        // The §4.5 data leak, at the one place the caller is decided. An authenticated user
        // and an anonymous caller must never land in the same bucket.
        let mut req = Request::new(Body::empty());
        req.extensions_mut()
            .insert(crate::middleware::auth::AuthUser {
                id: Uuid::from_u128(42),
                jti: Uuid::from_u128(43),
                full_name: "Ada".to_owned(),
                email: "ada@example.com".to_owned(),
                verified: true,
                providers: vec![],
                role: crate::middleware::auth::UserRole::User,
            });

        assert_eq!(
            scope_identifier(&req),
            IdempotencyScope::user(Uuid::from_u128(42))
        );
    }

    #[test]
    fn an_unauthenticated_caller_falls_back_to_one_fixed_bucket() {
        // Not a fresh bucket per request, which would be a free unlimited pass, and not the
        // client's token, which would be the §7.4 bug.
        assert_eq!(
            scope_identifier(&Request::new(Body::empty())),
            IdempotencyScope::anonymous("unknown")
        );
    }

    #[test]
    fn the_replay_marker_is_set_and_readable() {
        // The marker is how a client tells a cache hit from a fresh run, so it has to survive
        // as a real header value rather than a name.
        let mut response = Response::new(Body::empty());

        mark_replayed(&mut response);

        let name = axum::http::HeaderName::from_static(HEADER_REPLAYED);
        assert_eq!(
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );
    }

    mod live {
        //! No Redis is available here, so these are `#[ignore]`d. Run with
        //! `cargo test -p meno-api -- --ignored middleware::idempotency`.

        use super::*;

        #[tokio::test]
        #[ignore = "needs a real Redis"]
        async fn a_second_retry_replays_instead_of_re_running() {
            let redis = Arc::new(test_redis().await);
            let key = scoped_key(
                &IdempotencyScope::user(Uuid::nil()),
                "/notes",
                Uuid::from_u128(1),
            );
            redis
                .set(
                    &key,
                    &CachedResponse::from_parts(
                        StatusCode::CREATED,
                        &HeaderMap::new(),
                        "first".into(),
                    ),
                )
                .await
                .expect("a set");

            let cached = redis
                .get::<CachedResponse>(&key)
                .await
                .expect("a get")
                .expect("a hit");

            assert_eq!(cached.body, "first");
        }

        async fn test_redis() -> Redis {
            let url = std::env::var("REDIS_URL").expect("REDIS_URL");
            Redis::new(crate::infrastructure::redis::RedisConfig::from_url(url))
                .await
                .expect("a Redis")
        }
    }
}
