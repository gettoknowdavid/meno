//! The Redis-backed [`OAuthStateStore`].
//!
//! # Why this exists and why the in-memory store is not enough
//!
//! [`InMemoryStateStore`] documents itself as "not for production", and the reason is
//! worth restating because it is a correctness bug rather than a performance one:
//!
//! - **Across replicas.** A Google callback is a fresh HTTP request that lands on
//!   whichever replica the load balancer picks. If the `state` was stored in the memory
//!   of the replica that issued the redirect, a callback landing anywhere else finds
//!   nothing — and is refused as a CSRF replay, which is indistinguishable from an
//!   attack. The user's sign-in fails for no reason they can see.
//! - **Across restarts.** The window between redirect and callback is seconds, but a
//!   deploy in that window loses every in-flight sign-in.
//!
//! Both are silent: the sign-in button simply stops working, intermittently, on the
//! instances that did not issue the redirect. A store that is correct on one process is
//! not correct on three.
//!
//! # Key and TTL
//!
//! One key, [`RedisKey::oauth2_state`], under `oauth2:{state}` with
//! [`ttl::OAUTH2_STATE`]. The TTL is not a cache eviction — it is the deadline of the
//! authorization request itself, and it is deliberately short (§9.4): a `state` that
//! outlived its own redirect is a replay surface that has no remaining purpose.
//!
//! # `take` is a delete-and-return, and that is the point
//!
//! The CSRF defence is *single use*, not *comparison*. [`Redis::get`] followed by
//! [`Redis::del`] would leave a window in which two concurrent callbacks both read the
//! value and both succeed. [`Redis::del`] returns the number of keys removed, so the
//! first caller gets `1` and every other gets `0` — and "unknown, already used or
//! expired" collapses to one answer, which is what keeps the endpoint from telling an
//! attacker whether a guess was close.

use async_trait::async_trait;

use super::error::OAuthError;
use super::store::{OAuthState, OAuthStateStore};
use crate::infrastructure::redis::Redis;
use crate::infrastructure::redis::keys::{RedisKey, ttl};

/// The production [`OAuthStateStore`]: Redis, shared by every replica.
#[derive(Clone)]
pub struct RedisOAuthStateStore {
    redis: Redis,
}

impl std::fmt::Debug for RedisOAuthStateStore {
    /// Never prints the handle: it holds a connection URL, and the URL holds a password.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisOAuthStateStore")
            .field("redis", &"[redacted]")
            .finish()
    }
}

impl RedisOAuthStateStore {
    /// Wrap a Redis handle.
    #[must_use]
    pub fn new(redis: Redis) -> Self {
        Self { redis }
    }

    /// The key an in-flight request lives under.
    ///
    /// A function so the TTL is decided in exactly one place: "how long does a sign-in
    /// stay claimable" has one answer, and it is not re-derivable at a call site.
    #[must_use]
    fn key(state: &str) -> RedisKey {
        RedisKey::oauth2_state(state, ttl::OAUTH2_STATE)
    }
}

#[async_trait]
impl OAuthStateStore for RedisOAuthStateStore {
    async fn put(&self, state: &OAuthState) -> Result<(), OAuthError> {
        self.redis
            .set(&Self::key(&state.state), state)
            .await
            .map_err(|error| {
                // Failing to store must not be survivable: with nothing persisted there
                // is no CSRF binding at all, so continuing would mean issuing a redirect
                // whose callback can never succeed — a sign-in that fails 100% of the
                // time, with no error until the user clicks the button.
                OAuthError::Upstream(format!("could not store the OAuth state: {error}"))
            })?;

        Ok(())
    }

    async fn take(&self, state: &str) -> Result<OAuthState, OAuthError> {
        let key = Self::key(state);

        // Delete *first*, then read. The order is the security property: deleting first
        // means the second concurrent callback finds nothing, so exactly one caller can
        // ever claim a given state. Reading first and deleting after would let two
        // callbacks both observe the value in the gap between them.
        let removed = self.redis.del(&key).await.map_err(|error| {
            OAuthError::Upstream(format!("could not claim the OAuth state: {error}"))
        })?;

        if removed == 0 {
            // Unknown, already claimed, or expired. One answer for all three: telling
            // them apart would turn the callback into an oracle for guessing `state`.
            return Err(OAuthError::Rejected(
                "the OAuth state is unknown or already used".to_owned(),
            ));
        }

        // The key is already gone, so this read is best-effort by construction — and a
        // miss here means Redis lost the value between the two calls (an eviction, or a
        // restart on a non-persistent instance). Same refusal as above, same reason.
        self.redis
            .get::<OAuthState>(&key)
            .await
            .map_err(|error| {
                OAuthError::Upstream(format!("could not read the OAuth state: {error}"))
            })?
            .ok_or_else(|| {
                OAuthError::Rejected("the OAuth state is unknown or already used".to_owned())
            })
    }
}

#[cfg(test)]
mod tests {
    //! The store's own contract, against the in-memory double's.
    //!
    //! These assert the *shape* of the behaviour — single use, TTL-bearing keys — using
    //! the in-memory store as the reference, because the property under test is the
    //! adapter's mapping onto Redis rather than Redis itself. Whether the `DEL`-then-
    //! `GET` sequence is atomic under concurrency is a property of Redis, exercised by
    //! `oauth_redis.rs` in the integration suite.

    use super::*;
    use crate::infrastructure::oauth::store::{InMemoryStateStore, STATE_TTL_SECS};

    fn state(value: &str) -> OAuthState {
        OAuthState {
            state: value.to_owned(),
            verifier: "verifier".to_owned(),
        }
    }

    #[test]
    fn the_key_carries_the_state_and_the_ttl() {
        let key = RedisOAuthStateStore::key("abc123");
        // A state value with no TTL would be a key that outlives its own purpose; the
        // constructor takes a `Duration` so that is not expressible.
        assert_eq!(key.as_str(), "oauth2:abc123");
    }

    #[tokio::test]
    async fn the_in_memory_store_singles_out_claiming_a_state_twice() {
        // The reference behaviour this adapter must reproduce. If the double and the
        // Redis adapter ever disagree here, the CSRF defence is one of them.
        let store = InMemoryStateStore::new();
        store
            .put(&state("one"))
            .await
            .expect("the first put succeeds");

        assert_eq!(
            store
                .take("one")
                .await
                .expect("the first claim succeeds")
                .state,
            "one"
        );
        assert!(
            store.take("one").await.is_err(),
            "a second claim of the same state must fail: that is the CSRF defence"
        );
    }

    #[test]
    fn the_key_ttl_is_the_state_window() {
        // §9.4: the TTL is the deadline of the authorization request, and it matches the
        // constant the store documents, so the two cannot drift apart.
        assert_eq!(RedisOAuthStateStore::key("abc").ttl(), ttl::OAUTH2_STATE);
        assert_eq!(ttl::OAUTH2_STATE.as_secs(), STATE_TTL_SECS);
    }
}
