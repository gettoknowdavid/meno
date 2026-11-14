//! The chat module's endpoint, with its layers attached.
//!
//! One route: `GET /broadcasts/{id}/chat/messages`. Everything that *writes* arrives on
//! the socket ([`crate::routes::ws`]), because a chat client is already holding one and
//! a second round trip to send a message is a round trip that a live conversation pays
//! for on every line.
//!
//! # Why this is merged rather than nested
//!
//! The path lives under `/broadcasts`, which [`super::broadcasts`] already writes in
//! full and mounts with `.merge`. `nest("/broadcasts", …)` strips the prefix before the
//! inner router matches, so this table is spelled out completely too and merged at the
//! root as well — see [`super::build_routes`] for the axum 0.8 rule that forces it.
//!
//! # Layers
//!
//! The same pair as the broadcast reads: the auth guard, and the read rate-limit policy.
//! Not [`LimitPolicy::auth`], which is sized for endpoints that hash a password; a client
//! scrolling a chat history is not credential work, and applying the credential policy
//! here would lock a room out of its own backlog.

use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::get;

use crate::infrastructure::redis::Redis;
use crate::middleware::auth::{AuthState, auth_middleware};
use crate::middleware::rate_limit::{LimitPolicy, RateLimitState, rate_limit_middleware};
use crate::modules::chat::handlers;
use crate::modules::chat::state::ChatState;

/// Every path this table serves, written out.
///
/// Written out rather than derived from the router, because a list derived from the thing
/// it is meant to check cannot fail. [`crate::routes::tests`] asserts against it.
pub const CHAT_PATHS: &[&str] = &["/broadcasts/00000000-0000-0000-0000-000000000000/chat/messages"];

/// The chat sub-router, before [`super::build_routes`] lifts it to [`ChatState`].
pub fn routes(guard: AuthState, redis: Arc<Redis>) -> Router<ChatState> {
    Router::new()
        .route(
            "/broadcasts/{id}/chat/messages",
            get(handlers::get_messages),
        )
        .layer(from_fn_with_state(
            RateLimitState::new(Redis::clone(&redis), LimitPolicy::read()),
            rate_limit_middleware,
        ))
        .layer(from_fn_with_state(guard, auth_middleware))
}
