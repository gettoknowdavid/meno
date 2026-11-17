//! The Bible catalogue's endpoint, with its layer attached.
//!
//! # Public, and rate-limited
//!
//! There is no auth guard here, by design: the catalogue is the same for every caller, the
//! payloads are read from an unsigned CDN (§3.3), and a client that has never signed in
//! may still list what scripture exists. The only layer is the read limit — the same
//! `LimitPolicy::read()` every read route uses, which buckets an unauthenticated caller by
//! peer address.
//!
//! # Merged, not nested
//!
//! The path is written out in full (`/bible/translations`), like the broadcast and notes
//! tables, so the route table and this list spell the same string and `merge` is the only
//! mount that keeps it. `nest("/bible", …)` would need a relative path here and a second
//! place to look when asking what this module serves.

use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::get;

use crate::infrastructure::redis::Redis;
use crate::middleware::rate_limit::{LimitPolicy, RateLimitState, rate_limit_middleware};
use crate::modules::bible::handlers;
use crate::modules::bible::state::BibleState;

/// Every path this table serves, written out.
///
/// Written out rather than derived from the router, because a list derived from the thing
/// it is meant to check cannot fail. [`crate::routes::tests`] asserts against it.
pub const BIBLE_PATHS: &[&str] = &["/bible/translations"];

/// The catalogue sub-router, before [`crate::routes::build_routes`] lifts it to
/// [`BibleState`].
pub fn routes(redis: Arc<Redis>) -> Router<BibleState> {
    Router::new()
        .route("/bible/translations", get(handlers::get_translations))
        // Reads only, so the loose policy that fails open: a limiter outage must not hide
        // the catalogue, and the response is a few hundred bytes.
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::read()),
            rate_limit_middleware,
        ))
}
