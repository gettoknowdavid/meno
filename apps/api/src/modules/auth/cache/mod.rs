//! The access-token blocklist, and the trait it sits behind.
//!
//! # Why the blocklist lives in Redis (§4.7 item 5)
//!
//! Access tokens are self-contained and short-lived, so "revoke this token" cannot be a
//! claim inside them. The alternatives are a database check per authenticated request or
//! a blocklist; the blocklist wins because a round trip to Neon on every request is a
//! cost the whole product pays for a revocation that is rare.
//!
//! The caveat the plan asks to be recorded, and this module records on
//! [`RedisAuthCache`]: a Redis restart loses every entry, so a revoked access token
//! becomes usable again until its own `exp`. That window is bounded by
//! [`ttl::BLOCK_LIST`], which is why the TTL is *not* shorter than the access-token
//! lifetime — a key that expires before the token it revokes is a revocation that
//! silently stops working.
//!
//! # Every key carries a TTL (§9.4)
//!
//! Not a convention here: [`RedisKey::block_list`] requires a `Duration` argument and
//! the `Redis` handle reads the expiry off the key when writing. There is no code path
//! that can write a blocklist entry without one, which is the only way the rule is
//! enforceable rather than aspirational.

mod redis_store;

// As in `repository`: reachable from the `tests/` suite, absent from a production
// build.
#[cfg(any(test, feature = "test-support"))]
mod memory_store;

pub use redis_store::RedisAuthCache;

#[cfg(any(test, feature = "test-support"))]
pub use memory_store::InMemoryAuthCache;

use async_trait::async_trait;
use meno_core::Error as MenoError;
use uuid::Uuid;

/// The blocklist operations the token service needs.
///
/// # Errors
///
/// Every method returns [`MenoError`]. A blocklist that cannot be reached is
/// [`MenoError::Upstream`], **not** a silent "not revoked": the middleware turns it into
/// a 503, because failing open on an auth-adjacent path is the bug §4.4 is about.
#[async_trait]
pub trait AuthCache: Send + Sync + std::fmt::Debug {
    /// Record an access token's `jti` as revoked for `ttl_secs` more.
    ///
    /// `ttl_secs` must be the token's *remaining* lifetime. Writing the full lifetime
    /// would hold an entry for a token that already expired, and writing zero would
    /// write a key that never disappears — both are bugs, and the caller computes this
    /// from `exp`.
    async fn block_access_token(&self, jti: Uuid, ttl_secs: i64) -> Result<(), MenoError>;

    /// Whether this specific token has been revoked.
    async fn is_token_blocked(&self, jti: Uuid) -> Result<bool, MenoError>;

    /// Revoke every token issued to `user_id` within the access-token window.
    ///
    /// The marker is written at "now"; [`Self::is_user_tokens_blocked`] then answers
    /// "was this token issued before the marker". That is what makes "log out all
    /// devices" one write instead of one per token.
    async fn block_all_user_tokens(&self, user_id: Uuid, window_secs: i64)
    -> Result<(), MenoError>;

    /// Whether `user_id` had a logout-all marker written after `issued_at`.
    ///
    /// `issued_at` is the token's `iat`, in seconds since the epoch.
    async fn is_user_tokens_blocked(
        &self,
        user_id: Uuid,
        issued_at: i64,
    ) -> Result<bool, MenoError>;
}

/// The namespace prefix for blocklist keys.
///
/// Re-exported from [`crate::infrastructure::constants::BLOCKLIST_PREFIX`] rather than
/// spelled here, so there is one answer to "what namespace does a revocation live in".
pub use crate::infrastructure::constants::BLOCKLIST_PREFIX;

#[cfg(test)]
mod tests {
    //! The blocklist's semantics, checked against [`InMemoryAuthCache`].
    //!
    //! These are the properties the token service's tests rely on, so they are asserted
    //! against the double rather than assumed. In particular the "issued before the
    //! marker" rule, which is the difference between "log out all devices" working and
    //! silently doing nothing.

    use super::*;
    use std::sync::Mutex;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[tokio::test]
    async fn an_unblocked_token_is_not_blocked() {
        let cache = InMemoryAuthCache::new();
        assert!(!cache.is_token_blocked(id(1)).await.expect("a read"));
    }

    #[tokio::test]
    async fn a_blocked_token_is_blocked() {
        let cache = InMemoryAuthCache::new();
        cache
            .block_access_token(id(1), 900)
            .await
            .expect("blocking");
        assert!(cache.is_token_blocked(id(1)).await.expect("a read"));
    }

    #[tokio::test]
    async fn blocking_one_token_does_not_block_another() {
        // The obvious bug: a blocklist keyed by user rather than by jti would revoke
        // every device's token.
        let cache = InMemoryAuthCache::new();
        cache
            .block_access_token(id(1), 900)
            .await
            .expect("blocking");

        assert!(cache.is_token_blocked(id(1)).await.expect("a read"));
        assert!(!cache.is_token_blocked(id(2)).await.expect("a read"));
    }

    #[tokio::test]
    async fn a_logout_all_marker_only_covers_tokens_issued_before_it() {
        // The comparison that makes "log out all devices" work without enumerating
        // tokens: a token minted *after* the logout must survive it, or a user who
        // signs back in is signed out again by their own previous session.
        let cache = InMemoryAuthCache::new();
        let user = id(9);

        cache
            .block_all_user_tokens(user, 900)
            .await
            .expect("blocking");

        assert!(
            cache
                .is_user_tokens_blocked(user, 1_000)
                .await
                .expect("a read"),
            "a token from before the logout must be blocked"
        );
        assert!(
            !cache
                .is_user_tokens_blocked(user, 10_000_000_000)
                .await
                .expect("a read"),
            "a token issued after the logout must survive"
        );
    }

    #[tokio::test]
    async fn a_logout_all_marker_is_scoped_to_one_user() {
        let cache = InMemoryAuthCache::new();
        cache
            .block_all_user_tokens(id(1), 900)
            .await
            .expect("blocking");

        assert!(
            !cache
                .is_user_tokens_blocked(id(2), 1_000)
                .await
                .expect("a read")
        );
    }

    #[tokio::test]
    async fn a_non_positive_window_is_refused_rather_than_writing_a_key_that_never_expires() {
        // §9.4. A zero TTL is the unbounded-growth failure; the adapter must not be
        // handed one.
        let cache = InMemoryAuthCache::new();
        assert!(cache.block_all_user_tokens(id(1), 0).await.is_err());
        assert!(cache.block_access_token(id(1), 0).await.is_err());
    }

    #[tokio::test]
    async fn the_blocklist_prefix_is_the_one_the_middleware_uses() {
        // `middleware::auth::blocklist_key` and this module have to agree or a token
        // revoked by one is invisible to the other.
        assert_eq!(BLOCKLIST_PREFIX, "BL");
        let _ = Mutex::new(());
    }
}
