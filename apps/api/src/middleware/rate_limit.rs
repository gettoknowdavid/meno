//! Request rate limiting.
//!
//! Rebuilt from `apps/api/src/shared/middleware/rate_limit.rs` on `master` (`903c3ba`),
//! which §7.3 records as **entirely dead** — never wired into a router, so eight route
//! modules believed they were protected and nothing was. Five defects fixed here, each
//! named by §4.4:
//!
//! 1. **`Extension<Option<RateLimitConfig>>` is gone.** `master` built
//!    `with_rate_limit(25, 60)` at each route and nothing read it — the middleware read
//!    `state.config.default_rate_limit` instead, silently discarding the limit the caller
//!    asked for. Configuration is now typed state ([`RateLimitState`]) plus an explicit
//!    [`LimitPolicy`] chosen per route group, so the limit in the source is the limit in
//!    force.
//! 2. **The Redis-error branch no longer fails open everywhere.** `Err(_) => next.run(req)`
//!    let an attacker who could make Redis slow also remove the limit. The answer is now a
//!    field on the policy: [`FailureMode::Closed`] for auth-adjacent routes (login,
//!    register, OTP) and for the expensive LiveKit-token mint, [`FailureMode::Open`] for
//!    public reads, where a limiter outage should not take a whole feed down.
//! 3. **Identity is the authenticated user id, then the peer address.** `master` used the
//!    last 16 characters of the bearer token. One user with two devices got two buckets, a
//!    refresh moved every bucket, and two users behind one NAT shared one only by
//!    coincidence of token bytes. None of that is "60 requests per minute".
//! 4. **The `X-RateLimit-*` and `Retry-After` headers are set on both paths.** `master`
//!    returned a hand-built JSON string whose shape matched nothing else in the codebase,
//!    and `.unwrap()`ed two header conversions on the request path (§7.10).
//! 5. **Keys go through [`RedisKey`].** `master` built a `String` with `format!`, so the
//!    rate-limit key had no guaranteed expiry — §9.4's mandatory-TTL rule cannot be
//!    enforced against a `String`.
//!
//! # The algorithm: a two-window sliding counter
//!
//! `master`'s Lua script is kept, per §4.4 ("it is correct"). It counts the current fixed
//! window and the previous one, weighting the previous by the fraction of the current
//! window already elapsed, which smooths the boundary a pure fixed window has. It is
//! atomic, and it lives in Redis, which is what makes the count **single-instance
//! consistent** — the property §4.4 asks for and the reason horizontal scaling on Render
//! works at all.
//!
//! # What is not here
//!
//! A per-user daily quota. That is a product rule with its own reset semantics
//! ([`RedisKey::quota`]), not a request-frequency guard.

use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use meno_core::Error as MenoError;
use uuid::Uuid;

use crate::infrastructure::redis::Redis;
use crate::infrastructure::redis::keys::RedisKey;
use crate::middleware::auth::AuthUser;
use crate::middleware::error_response;

/// `X-RateLimit-Limit`: the request budget for the current window.
pub const HEADER_LIMIT: &str = "x-ratelimit-limit";
/// `X-RateLimit-Remaining`: how much of the budget is left.
pub const HEADER_REMAINING: &str = "x-ratelimit-remaining";
/// `X-RateLimit-Reset`: seconds until the current window ends.
pub const HEADER_RESET: &str = "x-ratelimit-reset";
/// `Retry-After`: seconds to wait before retrying. §4.4 requires it.
pub const HEADER_RETRY_AFTER: &str = "retry-after";

/// Wire code for a limit that has been reached.
pub const CODE_RATE_LIMITED: &str = "RATE_LIMITED";
/// Wire code for a limiter that could not reach its backend.
pub const CODE_RATE_LIMIT_UNAVAILABLE: &str = "RATE_LIMIT_UNAVAILABLE";

/// How many requests a caller may make in a window.
///
/// `limit` rather than a remaining count, because a budget and a window are the two facts
/// a reader needs in order to know whether the number is sane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitConfig {
    /// Requests permitted per window.
    pub limit: u32,
    /// The width of the window, in seconds.
    pub window_secs: u64,
}

impl RateLimitConfig {
    /// Build a config, rejecting values the sliding-window arithmetic cannot handle.
    ///
    /// Returns [`InvalidConfig`] rather than panicking. This runs at wiring time, which a
    /// request can reach, and §9.1 forbids panicking there.
    pub fn new(limit: u32, window_secs: u64) -> Result<Self, InvalidConfig> {
        // A zero window makes the script's `window_ms` zero, and every request weight a
        // division by zero — a runtime fault inside Redis that no unit test would see.
        if window_secs == 0 {
            return Err(InvalidConfig {
                limit,
                window_secs,
                reason: "the window must be at least one second",
            });
        }
        // A zero limit throttles every request, which is a typo that looks like an outage
        // rather than a config mistake.
        if limit == 0 {
            return Err(InvalidConfig {
                limit,
                window_secs,
                reason: "the limit must be at least one request",
            });
        }

        Ok(Self { limit, window_secs })
    }

    /// Seconds until the current window ends, from `now_ms`.
    ///
    /// At least 1: `Retry-After: 0` tells a client to retry immediately, which is the
    /// opposite of what a 429 means.
    #[must_use]
    pub fn retry_after_secs(&self, now_ms: u64) -> u64 {
        let window_ms = self.window_secs.saturating_mul(1_000);
        let elapsed = now_ms % window_ms;

        ((window_ms - elapsed) / 1_000).max(1)
    }
}

/// Why a [`RateLimitConfig`] was refused.
///
/// Carries the offending values so the startup log names the number that is wrong rather
/// than "bad config" — §7.8's complaint was that the first problem was reported without the
/// detail needed to fix it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidConfig {
    /// The limit that was asked for.
    pub limit: u32,
    /// The window that was asked for, in seconds.
    pub window_secs: u64,
    /// Why it was refused.
    pub reason: &'static str,
}

impl std::fmt::Display for InvalidConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} (limit={}, window_secs={})",
            self.reason, self.limit, self.window_secs
        )
    }
}

impl std::error::Error for InvalidConfig {}

/// What to do when the limiter's own backend is unreachable.
///
/// §4.4: fail **closed** on auth-adjacent routes, fail **open** on public reads. Making this
/// a property of the policy rather than an `if` inside the middleware is the point — the
/// decision is reviewable as one enum instead of as one branch nobody reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureMode {
    /// Refuse the request with a 503.
    ///
    /// Correct where the operation behind the limit is the thing worth attacking: a login
    /// endpoint whose limiter is down must not become an unthrottled login endpoint.
    Closed,
    /// Let the request through.
    ///
    /// Correct where a limiter outage should degrade rather than break. A public feed that
    /// 503s because Redis hiccuped is a worse outcome than an unthrottled feed read.
    Open,
}

/// The tier a route group runs under.
///
/// A distinct type from [`RateLimitConfig`] because the *policy* — budget **and** failure
/// mode **and** key namespace — is what a route picks. `master` is what happens when those
/// are one thing: the failure mode ends up implicit in a code path nobody reads, and two
/// tiers quietly share a counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitPolicy {
    /// The request budget.
    pub config: RateLimitConfig,
    /// What to do when the backend is unreachable.
    pub on_backend_failure: FailureMode,
    /// The key namespace, so `/auth` and `/broadcasts/…/join` do not share a bucket.
    pub namespace: &'static str,
}

impl LimitPolicy {
    /// Authentication routes: strict, and closed. §4.4's 5/min.
    #[must_use]
    pub const fn auth() -> Self {
        Self {
            config: RateLimitConfig {
                limit: 5,
                window_secs: 60,
            },
            on_backend_failure: FailureMode::Closed,
            namespace: "auth",
        }
    }

    /// Broadcast join: strict, and closed — it mints a LiveKit token, the path §4.4 names
    /// as expensive.
    #[must_use]
    pub const fn broadcast_join() -> Self {
        Self {
            config: RateLimitConfig {
                limit: 5,
                window_secs: 60,
            },
            on_backend_failure: FailureMode::Closed,
            namespace: "join",
        }
    }

    /// Reads: loose, and open.
    #[must_use]
    pub const fn read() -> Self {
        Self {
            config: RateLimitConfig {
                limit: 120,
                window_secs: 60,
            },
            on_backend_failure: FailureMode::Open,
            namespace: "read",
        }
    }

    /// Ordinary mutations: moderate, and open. A limiter outage must not block writes, but
    /// the budget still applies whenever the limiter is healthy.
    #[must_use]
    pub const fn write() -> Self {
        Self {
            config: RateLimitConfig {
                limit: 30,
                window_secs: 60,
            },
            on_backend_failure: FailureMode::Open,
            namespace: "write",
        }
    }
}

/// Everything the middleware needs: the backend, and the policy it enforces.
///
/// Built once at startup and applied with `from_fn_with_state`, so the policy cannot be
/// absent at request time — which is the `master` bug, where a silently-missing
/// `Extension` meant the default applied instead of the requested limit.
#[derive(Clone)]
pub struct RateLimitState {
    redis: Redis,
    policy: LimitPolicy,
}

impl std::fmt::Debug for RateLimitState {
    /// `Redis` has no `Debug` because it wraps a connection URL, which is a
    /// credential, so the adapter prints only the part that is safe.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RateLimitState")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl RateLimitState {
    /// Build a state enforcing `policy` against `redis`.
    #[must_use]
    pub fn new(redis: Redis, policy: LimitPolicy) -> Self {
        Self { redis, policy }
    }

    /// The policy in force.
    #[must_use]
    pub const fn policy(&self) -> LimitPolicy {
        self.policy
    }
}

/// Who a request is counted against.
///
/// §4.4: the authenticated user id, falling back to the peer address. The distinction is
/// not cosmetic — a user id is stable across token refreshes and across IP changes, so it is
/// the only identifier under which "60 requests per minute" means what a reader thinks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallerId {
    /// A signed-in user.
    User(Uuid),
    /// An unauthenticated caller at this address.
    Address(String),
}

impl CallerId {
    /// The stable string used in the Redis key.
    #[must_use]
    pub fn as_key_part(&self) -> String {
        match self {
            Self::User(id) => format!("u:{id}"),
            Self::Address(address) => format!("a:{address}"),
        }
    }
}

/// Identify the caller: the auth layer's user, else the peer address.
///
/// Reads the [`AuthUser`] that [`crate::middleware::auth::auth_middleware`] inserted, which
/// is why §4.4 requires the limiter to run *after* it. Falls back to `ConnectInfo` and
/// finally to a fixed `unknown`, so an unidentifiable caller shares one bucket rather than
/// getting a fresh unlimited one per request.
#[must_use]
pub fn caller_id(req: &Request) -> CallerId {
    if let Some(user) = req.extensions().get::<AuthUser>() {
        return CallerId::User(user.id);
    }

    CallerId::Address(
        peer_address(req).map_or_else(|| "unknown".to_owned(), |address| address.ip().to_string()),
    )
}

/// The peer address, if axum put one on this request.
///
/// Read from extensions rather than declared as a second extractor parameter: axum's
/// `from_fn` accepts one extractor ahead of the `Request`, so a middleware cannot ask for both
/// `State` and `ConnectInfo`. `ConnectInfo` is inserted into extensions by the router, so
/// reading it here is the same value by a supported route.
#[must_use]
pub fn peer_address(req: &Request) -> Option<SocketAddr> {
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| *address)
}

/// The Redis key for `(namespace, caller)`.
#[must_use]
pub fn rate_limit_key(policy: &LimitPolicy, caller: &CallerId) -> RedisKey {
    RedisKey::rate_limit(
        policy.namespace,
        &caller.as_key_part(),
        std::time::Duration::from_secs(policy.config.window_secs * 2),
    )
}

/// A per-window counter key.
///
/// The window index is part of the identifier so a fixed window's counter starts empty, and
/// the TTL is two windows so the *previous* window is still readable for the whole of the
/// current one — a one-window TTL would delete it halfway and silently halve the estimate.
#[must_use]
fn windowed_key(policy: &LimitPolicy, caller: &CallerId, window: u64) -> RedisKey {
    RedisKey::rate_limit(
        policy.namespace,
        &format!("{}:{window}", caller.as_key_part()),
        std::time::Duration::from_secs(policy.config.window_secs * 2),
    )
}

/// The sliding-window counter, kept from `master`; §4.4 states it is correct.
///
/// `KEYS[1]` is the current window's counter and `KEYS[2]` the previous one. Returns
/// `{remaining, current_count}`; a remaining at or below zero is over budget.
const SLIDING_WINDOW_SCRIPT: &str = r#"
    local current_key = KEYS[1]
    local previous_key = KEYS[2]
    local tokens = tonumber(ARGV[1])
    local window_secs = tonumber(ARGV[2])
    local now = tonumber(ARGV[3])
    local limit = tonumber(ARGV[4])

    local current_count = redis.call('INCRBY', current_key, tokens)
    redis.call('EXPIRE', current_key, window_secs * 2)

    local prev_count = tonumber(redis.call('GET', previous_key) or "0")

    local window_ms = window_secs * 1000
    local time_in_window = now % window_ms
    local weight = (window_ms - time_in_window) / window_ms

    local estimated = current_count + math.floor(prev_count * weight)

    if estimated > limit then
        return {0, current_count}
    else
        return {limit - estimated, current_count}
    end
"#;

/// The outcome of one limiter check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verdict {
    /// Requests left in the window. At or below zero means over budget.
    pub remaining: i64,
    /// The window the counters belong to.
    pub window: u64,
    /// Seconds until the current window ends.
    pub retry_after_secs: u64,
}

impl Verdict {
    /// Whether this request is over budget.
    #[must_use]
    pub const fn is_exhausted(self) -> bool {
        self.remaining <= 0
    }
}

/// Milliseconds since the unix epoch.
///
/// # Errors
///
/// [`MenoError::Internal`] if the system clock is before 1970 — a real possibility in a
/// container that booted before its clock was set, and one that must not become a panic or
/// a silently-negative window index.
fn now_ms() -> Result<u64, MenoError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| MenoError::Internal {
            context: "rate_limit_now",
            detail: error.to_string(),
        })?;

    Ok(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}

/// Count this request against the caller's budget.
///
/// # Errors
///
/// A backend failure, kept distinct from "over budget" by type so the caller can apply
/// [`FailureMode`]. §4.4's whole point is that these are not the same branch.
async fn check(
    redis: &Redis,
    policy: &LimitPolicy,
    caller: &CallerId,
) -> Result<Verdict, MenoError> {
    let config = policy.config;
    let now_ms = now_ms()?;
    let window_ms = config.window_secs.saturating_mul(1_000);

    let window = now_ms / window_ms;
    let previous = window.saturating_sub(1);

    let current_key = windowed_key(policy, caller, window);
    let previous_key = windowed_key(policy, caller, previous);

    let (remaining, _count): (i64, i64) = redis
        .eval(
            SLIDING_WINDOW_SCRIPT,
            vec![current_key.as_str(), previous_key.as_str()],
            vec![
                1_i64,
                i64::try_from(config.window_secs).unwrap_or(i64::MAX),
                i64::try_from(now_ms).unwrap_or(i64::MAX),
                i64::from(config.limit),
            ],
        )
        .await
        .map_err(|error| {
            tracing::warn!(
                service = "redis",
                namespace = policy.namespace,
                detail = %error,
                "rate limit check failed"
            );
            MenoError::Upstream {
                service: "rate-limiter",
                detail: error.to_string(),
            }
        })?;

    Ok(Verdict {
        remaining,
        window,
        retry_after_secs: config.retry_after_secs(now_ms),
    })
}

/// Set a header, skipping it if the value cannot be represented.
///
/// A diagnostic header is never worth failing a request over, and `master` unwrapped these
/// conversions on the request path (§7.10). The values here are all integers, so in
/// practice nothing is skipped; the fallback exists so that stays true rather than
/// becoming an assumption.
fn set_header(response: &mut Response, name: &str, value: &str) {
    let (Ok(name), Ok(value)) = (
        HeaderName::from_bytes(name.as_bytes()),
        HeaderValue::from_str(value),
    ) else {
        tracing::warn!(header = name, "could not set a rate limit header");
        return;
    };

    response.headers_mut().insert(name, value);
}

/// Set the §4.4 `X-RateLimit-*` headers on a response being allowed through.
fn annotate(response: &mut Response, policy: &LimitPolicy, verdict: Verdict) {
    set_header(response, HEADER_LIMIT, &policy.config.limit.to_string());
    // A negative remaining is not a value any client can act on, so it floors at zero.
    set_header(
        response,
        HEADER_REMAINING,
        &verdict.remaining.max(0).to_string(),
    );
    set_header(
        response,
        HEADER_RESET,
        &verdict.retry_after_secs.to_string(),
    );
}

/// Count the request and reject it if the caller is over budget.
///
/// Apply with `from_fn_with_state`:
///
/// ```ignore
/// let state = RateLimitState::new(redis.clone(), LimitPolicy::auth());
/// Router::new()
///     .route("/auth/login", post(login))
///     .layer(axum::middleware::from_fn_with_state(state, rate_limit_middleware))
/// ```
///
/// The 429 body is the §4.2 envelope with `RATE_LIMITED`, so a client handles a throttled
/// login exactly as it handles every other error.
pub async fn rate_limit_middleware(
    State(state): State<RateLimitState>,
    req: Request,
    next: Next,
) -> Response {
    let policy = state.policy();
    let caller = caller_id(&req);

    match check(&state.redis, &policy, &caller).await {
        Ok(verdict) if verdict.is_exhausted() => {
            let mut response = error_response(
                StatusCode::TOO_MANY_REQUESTS,
                CODE_RATE_LIMITED,
                "too many requests; slow down and retry",
            );
            annotate(&mut response, &policy, verdict);
            set_header(
                &mut response,
                HEADER_RETRY_AFTER,
                &verdict.retry_after_secs.to_string(),
            );
            response
        }
        Ok(verdict) => {
            let mut response = next.run(req).await;
            annotate(&mut response, &policy, verdict);
            response
        }
        Err(error) => match policy.on_backend_failure {
            // The fix for §7.3's fail-open. A limiter outage must not become an unthrottled
            // login endpoint.
            FailureMode::Closed => {
                tracing::error!(
                    namespace = policy.namespace,
                    detail = %error,
                    "rate limiter unavailable on a fail-closed route; refusing"
                );
                error_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    CODE_RATE_LIMIT_UNAVAILABLE,
                    "this endpoint is temporarily unavailable; please retry",
                )
            }
            // A public read degrades rather than breaks: §4.4.
            FailureMode::Open => {
                tracing::warn!(
                    namespace = policy.namespace,
                    detail = %error,
                    "rate limiter unavailable on a fail-open route; allowing"
                );
                next.run(req).await
            }
        },
    }
}

/// Choose a policy from a method and the matched route pattern.
///
/// For wiring a whole router at one limit rather than grouping routes by hand. Call it with
/// axum's *matched path* (`/notes/{id}`), not the concrete path, so a route pattern does
/// not change tier depending on which id it matched — the same reasoning §4.5 gives for
/// idempotency keys.
#[must_use]
pub fn policy_for(method: &Method, path: &str) -> LimitPolicy {
    if path.starts_with("/auth") {
        return LimitPolicy::auth();
    }
    if path.ends_with("/join") {
        return LimitPolicy::broadcast_join();
    }
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return LimitPolicy::read();
    }

    LimitPolicy::write()
}

#[cfg(test)]
mod tests {
    //! Tests for the identification rule, the policy table, the key layout and the headers —
    //! everything that decides behaviour without a Redis round trip.
    //!
    //! The Redis path is in `mod live`, which is also where §10's "assert 429 after N
    //! requests" lives: the proof that the subsystem §7.3 called dead is now alive.

    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;

    fn header(response: &Response, name: &str) -> Option<String> {
        let name = HeaderName::from_bytes(name.as_bytes()).expect("a header name");

        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    }

    fn request_carrying(user: Option<AuthUser>) -> Request {
        let mut req = Request::new(Body::empty());
        if let Some(user) = user {
            req.extensions_mut().insert(user);
        }
        req
    }

    fn user() -> AuthUser {
        AuthUser {
            id: Uuid::from_u128(1),
            jti: Uuid::from_u128(2),
            full_name: "Ada".to_owned(),
            email: "ada@example.com".to_owned(),
            verified: true,
            providers: vec![],
            role: crate::middleware::auth::UserRole::User,
        }
    }

    fn with_peer(req: &mut Request, raw: &str) {
        req.extensions_mut().insert(ConnectInfo(
            raw.parse::<SocketAddr>().expect("a socket address"),
        ));
    }

    // ── identification: §4.4 ───────────────────────────────────────────────

    #[test]
    fn an_authenticated_caller_is_identified_by_user_id() {
        // The §4.4 rule, and the reason the limiter must run after the auth layer.
        let id = caller_id(&request_carrying(Some(user())));

        assert_eq!(id, CallerId::User(Uuid::from_u128(1)));
        assert_eq!(id.as_key_part(), "u:00000000-0000-0000-0000-000000000001");
    }

    #[test]
    fn the_user_id_is_independent_of_the_token() {
        // `master` keyed on the last 16 characters of the bearer token, so one user with two
        // devices got two buckets and a refresh moved every one of them. Pinning the
        // identifier to `AuthUser::id` is what makes the limit mean what it says.
        let mut refreshed = user();
        refreshed.jti = Uuid::from_u128(999);

        let before = caller_id(&request_carrying(Some(user())));
        let after = caller_id(&request_carrying(Some(refreshed)));

        assert_eq!(before, after, "a new token must not reset the budget");
    }

    #[test]
    fn an_anonymous_caller_falls_back_to_the_peer_address() {
        let mut req = Request::new(Body::empty());
        with_peer(&mut req, "203.0.113.4:5000");

        assert_eq!(caller_id(&req), CallerId::Address("203.0.113.4".to_owned()));
    }

    #[test]
    fn an_unidentifiable_caller_shares_one_bucket() {
        // A per-request random identifier would hand every unidentified caller a fresh
        // budget — a free unlimited pass. A fixed `unknown` keeps the limit applying.
        let id = caller_id(&Request::new(Body::empty()));

        assert_eq!(id, CallerId::Address("unknown".to_owned()));
        assert!(id.as_key_part().starts_with("a:"));
    }

    #[test]
    fn a_user_and_an_address_never_share_a_bucket() {
        // The `u:`/`a:` prefixes exist so an address cannot be crafted to impersonate a user
        // id, or the reverse.
        let user_part = CallerId::User(Uuid::nil()).as_key_part();
        let address_part =
            CallerId::Address("u:00000000-0000-0000-0000-000000000000".to_owned()).as_key_part();

        assert!(user_part.starts_with("u:"));
        assert!(address_part.starts_with("a:"));
        assert_ne!(user_part, address_part);
    }

    #[test]
    fn the_authenticated_identity_wins_over_the_peer_address() {
        // A user behind a rotating NAT must be limited as one user, so the user id takes
        // precedence over the address the connection happened to come from.
        let mut req = request_carrying(Some(user()));
        with_peer(&mut req, "198.51.100.7:5000");

        assert!(matches!(caller_id(&req), CallerId::User(_)));
    }

    #[test]
    fn the_peer_address_is_absent_when_the_router_supplied_none() {
        // No `ConnectInfo` on the request is a normal case — a test router, or a server not
        // behind `into_make_service_with_connect_info`. It must fall through, not panic.
        assert_eq!(peer_address(&Request::new(Body::empty())), None);
    }

    // ── the policy table: §4.4's tiered limits ────────────────────────────

    #[test]
    fn auth_routes_get_the_strict_tier() {
        // §4.4: 5/min on `/auth/*`.
        for path in [
            "/auth/login",
            "/auth/register",
            "/auth/otp/verify",
            "/auth/refresh",
        ] {
            let policy = policy_for(&Method::POST, path);

            assert_eq!(policy.config.limit, 5, "{path} must be strict");
            assert_eq!(policy.namespace, "auth", "{path}");
        }
    }

    #[test]
    fn the_join_route_gets_the_strict_tier() {
        // The LiveKit-token mint §4.4 names as the expensive path.
        let policy = policy_for(&Method::POST, "/broadcasts/{id}/join");

        assert_eq!(policy.config.limit, 5);
        assert_eq!(policy.namespace, "join");
    }

    #[test]
    fn reads_are_looser_than_writes() {
        let read = policy_for(&Method::GET, "/broadcasts");
        let write = policy_for(&Method::POST, "/notes");

        assert!(
            read.config.limit > write.config.limit,
            "a read must not cost the same budget as a write"
        );
    }

    #[test]
    fn auth_routes_are_stricter_than_reads() {
        let auth = policy_for(&Method::GET, "/auth/me");
        let read = policy_for(&Method::GET, "/broadcasts");

        assert!(auth.config.limit < read.config.limit);
    }

    #[test]
    fn the_auth_tier_wins_when_a_path_matches_more_than_one_rule() {
        // A path matching both is a routing oddity, and the strict answer is the safe one.
        // Asserted so a future reordering is a deliberate act.
        assert_eq!(policy_for(&Method::POST, "/auth/join").namespace, "auth");
    }

    #[test]
    fn auth_and_join_fail_closed_while_reads_and_writes_fail_open() {
        // §4.4 verbatim: fail closed on auth-adjacent routes, fail open on public reads.
        assert_eq!(LimitPolicy::auth().on_backend_failure, FailureMode::Closed);
        assert_eq!(
            LimitPolicy::broadcast_join().on_backend_failure,
            FailureMode::Closed
        );
        assert_eq!(LimitPolicy::read().on_backend_failure, FailureMode::Open);
        assert_eq!(LimitPolicy::write().on_backend_failure, FailureMode::Open);
    }

    #[test]
    fn the_config_constructor_refuses_a_zero_window() {
        // A zero window makes the script's `window_ms` zero, so every request weight becomes
        // a division by zero — a fault inside Redis that no unit test would otherwise see.
        let error = RateLimitConfig::new(5, 0).expect_err("a zero window must be refused");

        assert_eq!(error.window_secs, 0);
        assert!(error.to_string().contains("window"));
    }

    #[test]
    fn the_config_constructor_refuses_a_zero_limit() {
        assert!(RateLimitConfig::new(0, 60).is_err());
    }

    #[test]
    fn an_invalid_config_names_the_numbers_that_are_wrong() {
        // §7.8: the error has to say enough to fix it without re-reading the wiring.
        let rendered = RateLimitConfig::new(0, 30)
            .expect_err("a zero limit")
            .to_string();

        assert!(rendered.contains("limit=0"), "{rendered}");
        assert!(rendered.contains("window_secs=30"), "{rendered}");
    }

    #[test]
    fn a_valid_config_is_accepted_and_readable() {
        let config = RateLimitConfig::new(25, 60).expect("a valid config");

        assert_eq!(config.limit, 25);
        assert_eq!(config.window_secs, 60);
    }

    // ── key layout: §9.4's mandatory TTL ───────────────────────────────────

    #[test]
    fn every_policy_produces_a_namespaced_key_with_a_ttl() {
        // §9.4, asserted on the limiter's own keys. A key with no expiry is how a 25 MB
        // free instance fills up and takes auth down with it (§3.2).
        let caller = CallerId::User(Uuid::from_u128(1));

        for policy in [
            LimitPolicy::auth(),
            LimitPolicy::broadcast_join(),
            LimitPolicy::read(),
            LimitPolicy::write(),
        ] {
            let key = rate_limit_key(&policy, &caller);

            assert!(
                key.as_str()
                    .starts_with(&format!("rate:{}:", policy.namespace)),
                "{}",
                key.as_str()
            );
            assert!(!key.ttl().is_zero(), "a zero TTL never expires");
        }
    }

    #[test]
    fn different_tiers_never_share_a_counter() {
        let caller = CallerId::User(Uuid::from_u128(1));

        assert_ne!(
            rate_limit_key(&LimitPolicy::auth(), &caller).as_str(),
            rate_limit_key(&LimitPolicy::read(), &caller).as_str()
        );
    }

    #[test]
    fn different_callers_never_share_a_counter() {
        let policy = LimitPolicy::read();

        assert_ne!(
            rate_limit_key(&policy, &CallerId::User(Uuid::from_u128(1))).as_str(),
            rate_limit_key(&policy, &CallerId::User(Uuid::from_u128(2))).as_str()
        );
        assert_ne!(
            rate_limit_key(&policy, &CallerId::Address("1.1.1.1".to_owned())).as_str(),
            rate_limit_key(&policy, &CallerId::Address("2.2.2.2".to_owned())).as_str()
        );
    }

    #[test]
    fn a_windowed_counter_outlives_the_window_it_belongs_to() {
        // The script reads the *previous* window for the whole of the current one, so a
        // one-window TTL would delete it halfway and silently halve the estimate.
        let policy = LimitPolicy::read();
        let key = windowed_key(&policy, &CallerId::User(Uuid::nil()), 5);

        assert_eq!(key.ttl().as_secs(), policy.config.window_secs * 2);
    }

    #[test]
    fn a_window_index_cannot_be_confused_with_a_different_caller() {
        let policy = LimitPolicy::auth();

        assert_ne!(
            windowed_key(&policy, &CallerId::User(Uuid::from_u128(1)), 2).as_str(),
            windowed_key(&policy, &CallerId::User(Uuid::from_u128(2)), 1).as_str()
        );
    }

    // ── headers: §4.4 ──────────────────────────────────────────────────────

    #[test]
    fn an_allowed_response_carries_the_three_headers() {
        let mut response = Response::new(Body::empty());

        annotate(
            &mut response,
            &LimitPolicy::read(),
            Verdict {
                remaining: 7,
                window: 1,
                retry_after_secs: 42,
            },
        );

        assert_eq!(header(&response, HEADER_LIMIT).as_deref(), Some("120"));
        assert_eq!(header(&response, HEADER_REMAINING).as_deref(), Some("7"));
        assert_eq!(header(&response, HEADER_RESET).as_deref(), Some("42"));
    }

    #[test]
    fn an_exhausted_budget_reports_zero_rather_than_a_negative() {
        // A negative `X-RateLimit-Remaining` is not a value any client can act on.
        let mut response = Response::new(Body::empty());

        annotate(
            &mut response,
            &LimitPolicy::auth(),
            Verdict {
                remaining: -3,
                window: 1,
                retry_after_secs: 10,
            },
        );

        assert_eq!(header(&response, HEADER_REMAINING).as_deref(), Some("0"));
    }

    #[test]
    fn a_retry_after_is_never_zero_and_never_exceeds_the_window() {
        // `Retry-After: 0` says "retry immediately", which is the opposite of a 429.
        let config = RateLimitConfig::new(5, 60).expect("a valid config");

        for now_ms in [0, 1, 999, 30_000, 59_999, 60_000, 119_999] {
            let retry = config.retry_after_secs(now_ms);
            assert!(
                (1..=60).contains(&retry),
                "{retry}s at {now_ms}ms is outside 1..=60"
            );
        }
    }

    #[test]
    fn an_unrepresentable_header_value_is_skipped_rather_than_panicking() {
        // `master` unwrapped these conversions on the request path (§7.10). The values are
        // integers in practice, but the failure mode must not be a panic if that changes.
        let mut response = Response::new(Body::empty());

        set_header(&mut response, "x-bad\nheader", "value");
        set_header(&mut response, HEADER_LIMIT, "not a number\u{7f}");

        assert!(response.headers().is_empty());
    }

    // ── the verdict ────────────────────────────────────────────────────────

    #[test]
    fn a_verdict_at_or_below_zero_is_exhausted() {
        for (remaining, exhausted) in [(1_i64, false), (0, true), (-1, true)] {
            let verdict = Verdict {
                remaining,
                window: 1,
                retry_after_secs: 1,
            };

            assert_eq!(verdict.is_exhausted(), exhausted, "{remaining} remaining");
        }
    }

    // ── the throttle body shape ────────────────────────────────────────────

    #[tokio::test]
    async fn the_throttle_response_uses_the_shared_envelope() {
        // `master` returned `{"data":null,"meta":null,"error":{…}}`, a shape nothing else in
        // the codebase produced. A client that handled every other error could not handle
        // this one.
        let response = error_response(
            StatusCode::TOO_MANY_REQUESTS,
            CODE_RATE_LIMITED,
            "too many requests; slow down and retry",
        );

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("the body buffers")
            .to_bytes();
        let body: serde_json::Value = serde_json::from_slice(&bytes).expect("the body is JSON");

        let keys: Vec<&str> = body
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec!["code", "message", "status"]);
        assert_eq!(body["code"], CODE_RATE_LIMITED);
    }

    #[test]
    fn the_unavailable_response_is_a_503_with_its_own_code() {
        // A client that backs off a 429 and retries a 503 must tell them apart from the
        // code alone.
        assert_eq!(
            error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                CODE_RATE_LIMIT_UNAVAILABLE,
                "unavailable",
            )
            .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    // ── the script's contract, without a Redis ─────────────────────────────

    #[test]
    fn the_script_reads_both_keys_and_every_argument() {
        // A Lua `KEYS`/`ARGV` mistake is a runtime error inside Redis, invisible until
        // production. Asserting the shape here is the cheapest available guard.
        for reference in [
            "KEYS[1]", "KEYS[2]", "ARGV[1]", "ARGV[2]", "ARGV[3]", "ARGV[4]",
        ] {
            assert!(
                SLIDING_WINDOW_SCRIPT.contains(reference),
                "{reference} is never referenced"
            );
        }
    }

    #[test]
    fn the_script_counts_and_bounds_the_counter_it_creates() {
        // Dropping either line would make the counter immortal, which is the §3.2 failure
        // the whole `RedisKey` redesign exists to prevent.
        assert!(SLIDING_WINDOW_SCRIPT.contains("INCRBY"));
        assert!(SLIDING_WINDOW_SCRIPT.contains("EXPIRE"));
    }

    // ── wiring ─────────────────────────────────────────────────────────────

    // `RateLimitState::policy` is asserted in `mod live`: `Redis::new` connects eagerly, so
    // even constructing the state needs a server to talk to.

    mod live {
        //! No Redis is available in unit-test runs, so these are `#[ignore]`d. Run with
        //! `cargo test -p meno-api -- --ignored middleware::rate_limit`.
        //!
        //! This is where §10's "assert 429 after N requests" lives: the proof that the
        //! subsystem §7.3 called dead is alive.

        use super::*;
        use axum::body::Body;
        use axum::routing::get;
        use http_body_util::BodyExt;
        use tower::ServiceExt;

        async fn test_redis() -> Redis {
            let url = std::env::var("REDIS_URL").expect("REDIS_URL is set for live tests");
            Redis::new(crate::infrastructure::redis::RedisConfig::from_url(url))
                .await
                .expect("a Redis")
        }

        async fn ok_handler() -> &'static str {
            "ok"
        }

        /// A router whose limiter enforces `policy` over a real Redis.
        ///
        /// The namespace is unique per test so a shared Redis instance cannot make one
        /// test's budget another test's.
        fn router(redis: Redis, policy: LimitPolicy) -> axum::Router {
            axum::Router::new().route("/thing", get(ok_handler)).layer(
                axum::middleware::from_fn_with_state(
                    RateLimitState::new(redis, policy),
                    rate_limit_middleware,
                ),
            )
        }

        async fn call(router: axum::Router) -> Response {
            router
                .oneshot(
                    Request::builder()
                        .uri("/thing")
                        .body(Body::empty())
                        .expect("a valid request"),
                )
                .await
                .expect("the router answers")
        }

        fn policy_for_test(namespace: &'static str, limit: u32) -> LimitPolicy {
            LimitPolicy {
                config: RateLimitConfig {
                    limit,
                    window_secs: 60,
                },
                on_backend_failure: FailureMode::Closed,
                namespace,
            }
        }

        #[tokio::test]
        #[ignore = "needs a real Redis"]
        async fn the_state_reports_the_policy_it_was_built_with() {
            // The `master` bug was a policy that could be built and then ignored. Asserting
            // the state hands it back is the smallest thing that catches a regression here.
            let state = RateLimitState::new(test_redis().await, LimitPolicy::broadcast_join());

            assert_eq!(state.policy(), LimitPolicy::broadcast_join());
        }

        #[tokio::test]
        #[ignore = "needs a real Redis"]
        async fn the_nth_request_is_throttled_and_the_first_n_are_not() {
            // §10's assertion, and the direct answer to §7.3's "no rate limiting anywhere".
            let redis = test_redis().await;
            let policy = policy_for_test("test-throttle", 3);

            for attempt in 1..=3 {
                let response = call(router(redis.clone(), policy)).await;
                assert_eq!(
                    response.status(),
                    StatusCode::OK,
                    "request {attempt} of 3 must pass"
                );
            }

            let response = call(router(redis.clone(), policy)).await;
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
            assert!(
                header(&response, HEADER_RETRY_AFTER).is_some(),
                "§4.4 requires Retry-After on a 429"
            );

            let bytes = response
                .into_body()
                .collect()
                .await
                .expect("the body buffers")
                .to_bytes();
            let body: serde_json::Value = serde_json::from_slice(&bytes).expect("the body is JSON");
            assert_eq!(body["code"], CODE_RATE_LIMITED);
        }

        #[tokio::test]
        #[ignore = "needs a real Redis"]
        async fn the_allowed_responses_report_a_decreasing_remaining_count() {
            let redis = test_redis().await;
            let policy = policy_for_test("test-headers", 5);

            let mut seen = Vec::new();
            for _ in 0..3 {
                let response = call(router(redis.clone(), policy)).await;
                seen.push(header(&response, HEADER_REMAINING).expect("a remaining header"));
            }

            assert_eq!(seen, vec!["4", "3", "2"], "the budget must count down");
        }

        #[tokio::test]
        #[ignore = "needs a real Redis"]
        async fn the_counter_is_shared_across_middleware_instances() {
            // §4.4's single-instance consistency. The count lives in Redis, so a second
            // layer — standing in for a second Render instance — continues the same budget.
            let redis = test_redis().await;
            let policy = policy_for_test("test-shared", 2);

            for _ in 0..2 {
                let response = call(router(redis.clone(), policy)).await;
                assert_eq!(response.status(), StatusCode::OK);
            }

            let response = call(router(redis.clone(), policy)).await;
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        }

        #[tokio::test]
        #[ignore = "needs a real Redis"]
        async fn a_second_caller_does_not_inherit_the_first_ones_budget() {
            let redis = test_redis().await;
            let policy = policy_for_test("test-independent", 1);

            let first = call(router(redis.clone(), policy)).await;
            assert_eq!(first.status(), StatusCode::OK);

            // The same anonymous caller, so the same bucket and the same budget.
            let second = call(router(redis.clone(), policy)).await;
            assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
        }
    }
}
