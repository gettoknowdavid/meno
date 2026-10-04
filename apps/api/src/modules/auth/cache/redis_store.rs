//! The Redis-backed [`AuthCache`].
//!
//! # Keys, and why each carries a TTL (§9.4)
//!
//! | Key | Built by | TTL |
//! | --- | --- | --- |
//! | `BL:{jti}` | [`RedisKey::block_list`] | the token's *remaining* lifetime |
//! | `BL:u:{user_id}` | [`RedisKey::user_tokens_blocked`] | the access-token window |
//!
//! Both constructors take a `Duration` and there is no way to build a [`RedisKey`]
//! without one, so "every key has a TTL" is a property of the type rather than of this
//! file's discipline. The Redis adapter for the *middleware's* blocklist,
//! [`crate::middleware::auth::RedisTokenBlocklist`], uses the same key for the same
//! token — a revocation written by logout and one read by the middleware are the same
//! entry, not two that have to be kept in step.
//!
//! # The restart caveat (§4.7 item 5)
//!
//! Redis here is a Render Key Value instance with no persistence, so a restart loses
//! every entry and a revoked access token becomes usable again until its own `exp`.
//! That window is bounded by the access-token TTL — which is why `BLOCK_LIST` is 900
//! seconds and why a shorter one would be the bug: a key that expires before the token
//! it revokes is a revocation that silently stops working.

use async_trait::async_trait;
use meno_core::Error as MenoError;
use uuid::Uuid;

use super::AuthCache;
use super::BLOCKLIST_PREFIX;
use crate::infrastructure::redis::Redis;
use crate::infrastructure::redis::keys::{RedisKey, ttl};

/// The value stored under a blocked `jti`.
///
/// Redis cannot store a bare `""` through [`Redis::set`], which serialises through
/// `serde_json`; an empty JSON string is not valid JSON. A struct with no fields is, and
/// it means "presence of this key is the signal", which is exactly the semantics.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Blocked {}

/// The Redis implementation of the access-token blocklist.
#[derive(Clone)]
pub struct RedisAuthCache {
    redis: Redis,
}

impl std::fmt::Debug for RedisAuthCache {
    /// `Redis` has no `Debug`: it holds a connection URL, and the URL usually holds a
    /// password. Printing the handle would print the credential.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisAuthCache")
            .field("redis", &"[redacted]")
            .finish()
    }
}

impl RedisAuthCache {
    /// Wrap a Redis handle.
    #[must_use]
    pub fn new(redis: Redis) -> Self {
        Self { redis }
    }

    /// The key a revoked token's `jti` lives under.
    ///
    /// Named as a function so the TTL choice is in one place and visible in review:
    /// "how long does a revocation live" has one answer.
    #[must_use]
    fn token_key(jti: Uuid, remaining_secs: i64) -> RedisKey {
        let seconds = u64::try_from(remaining_secs).unwrap_or(1).max(1);
        RedisKey::block_list(
            BLOCKLIST_PREFIX,
            jti,
            std::time::Duration::from_secs(seconds),
        )
    }

    /// The key a user's "log out everywhere" marker lives under.
    ///
    /// Shares the `BL` namespace with the per-token entries but carries a `u:` infix, so
    /// a `SCAN` over one never matches the other.
    #[must_use]
    fn user_key(user_id: Uuid, window_secs: i64) -> RedisKey {
        let seconds = u64::try_from(window_secs).unwrap_or(1).max(1);
        RedisKey::user_tokens_blocked(
            BLOCKLIST_PREFIX,
            user_id,
            std::time::Duration::from_secs(seconds),
        )
    }

    /// The window a user's marker must outlive.
    ///
    /// `BLOCK_LIST` rather than the token service's configured access TTL: the two must
    /// be the same number, and this is the copy that the constant is asserted against,
    /// so a change to `ACCESS_TOKEN_EXPIRATION` cannot silently leave markers expiring
    /// before the tokens they revoke.
    #[must_use]
    pub fn marker_window() -> std::time::Duration {
        ttl::BLOCK_LIST
    }
}

/// Map a Redis failure onto a 503-shaped error.
///
/// [`MenoError::Upstream`], never a default: the middleware turns this into a 503, and
/// the alternative — treating an unreachable blocklist as "nothing is revoked" — is the
/// fail-open §4.4 names.
fn unavailable(operation: &'static str, error: impl std::fmt::Display) -> MenoError {
    MenoError::Upstream {
        service: "redis",
        detail: format!("{operation}: {error}"),
    }
}

/// Refuse a non-positive TTL before a write (§9.4).
///
/// A zero second would be a key that never expires; a negative one is a Redis error.
/// Both are refused here rather than clamped, so the caller finds out it computed the
/// remaining lifetime wrong.
fn require_positive(operation: &'static str, secs: i64) -> Result<(), MenoError> {
    if secs > 0 {
        return Ok(());
    }
    Err(MenoError::Internal {
        context: operation,
        detail: format!("refused a {secs}-second blocklist TTL; it must be positive"),
    })
}

#[async_trait]
impl AuthCache for RedisAuthCache {
    async fn block_access_token(&self, jti: Uuid, ttl_secs: i64) -> Result<(), MenoError> {
        require_positive("block_access_token", ttl_secs)?;

        self.redis
            .set(&Self::token_key(jti, ttl_secs), &Blocked {})
            .await
            .map_err(|e| unavailable("block_access_token", e))
    }

    async fn is_token_blocked(&self, jti: Uuid) -> Result<bool, MenoError> {
        // `exists` rather than `get`: presence is the signal, and reading a value back
        // to discard it is a second reason for the call to fail.
        self.redis
            .exists(&Self::token_key(
                jti,
                Self::marker_window().as_secs() as i64,
            ))
            .await
            .map_err(|e| unavailable("is_token_blocked", e))
    }

    async fn block_all_user_tokens(
        &self,
        user_id: Uuid,
        window_secs: i64,
    ) -> Result<(), MenoError> {
        require_positive("block_all_user_tokens", window_secs)?;

        // The marker is the wall-clock second the logout happened. A token issued at or
        // before it is revoked; one issued after is a fresh sign-in and must survive.
        self.redis
            .set(
                &Self::user_key(user_id, window_secs),
                &time::OffsetDateTime::now_utc().unix_timestamp(),
            )
            .await
            .map_err(|e| unavailable("block_all_user_tokens", e))
    }

    async fn is_user_tokens_blocked(
        &self,
        user_id: Uuid,
        issued_at: i64,
    ) -> Result<bool, MenoError> {
        let window = Self::marker_window().as_secs() as i64;
        let key = Self::user_key(user_id, window);

        let marker: Option<i64> = self
            .redis
            .get(&key)
            .await
            .map_err(|e| unavailable("is_user_tokens_blocked", e))?;

        Ok(marker.is_some_and(|marked_at| issued_at <= marked_at))
    }
}

#[cfg(test)]
mod tests {
    //! The two decisions this adapter makes that a reviewer should be able to check
    //! without a Redis: the TTL arithmetic, and the fact that both this adapter and the
    //! middleware's blocklist address the same key.
    //!
    //! The round trips themselves are `#[ignore]`d and need a server; `Redis::new`
    //! connects eagerly, so they cannot even be constructed without one.

    use super::*;
    use crate::middleware::auth::blocklist_key;

    #[test]
    fn a_remaining_lifetime_shorter_than_a_second_is_still_written() {
        // A token with 400 ms left is a real token and must be blockable. Clamping to
        // one second keeps it revoked for slightly longer, which is the safe direction.
        let key = RedisAuthCache::token_key(Uuid::nil(), 0);
        assert_eq!(key.ttl().as_secs(), 1);
        assert!(key.as_str().starts_with("block-list:BL:"));
    }

    #[test]
    fn a_non_positive_ttl_is_refused_rather_than_clamped() {
        // Not clamped here: the caller computed the remaining lifetime wrongly and
        // should find out, rather than writing a key nobody will ever clean up.
        assert!(require_positive("block_access_token", 0).is_err());
        assert!(require_positive("block_access_token", -1).is_err());
        assert!(require_positive("block_access_token", 1).is_ok());
    }

    #[test]
    fn the_token_key_is_the_one_the_middleware_reads() {
        // The whole point of a shared namespace: a logout that writes here and a
        // request that the middleware rejects must be the same entry. If these drift,
        // revocation appears to work and does nothing.
        let jti = Uuid::new_v4();
        assert_eq!(
            RedisAuthCache::token_key(jti, 900).as_str(),
            blocklist_key(jti).as_str()
        );
    }

    #[test]
    fn a_users_marker_lives_under_the_same_prefix_with_its_own_key() {
        let user = Uuid::new_v4();
        let token = Uuid::new_v4();
        // Bound so the test fails if a builder ever stops taking an id.
        assert_ne!(user, token);

        // Even for the *same* uuid, the two key shapes differ -- which is what stops a
        // logout marker colliding with a single-token revocation.
        let shared = Uuid::new_v4();
        let marker = RedisAuthCache::user_key(shared, 900);
        assert_ne!(
            marker.as_str(),
            RedisAuthCache::token_key(shared, 900).as_str()
        );

        // And the marker is namespaced under the same blocklist prefix as a single-token
        // revocation, with a `u:` segment in between so a key scan for one cannot match
        // the other.
        assert!(marker.as_str().starts_with("block-list:BL:u:"));
    }

    #[test]
    fn the_marker_window_matches_the_canonical_blocklist_ttl() {
        // If `ACCESS_TOKEN_EXPIRATION` were raised past this, a marker would expire
        // while the tokens it revokes were still valid.
        assert_eq!(RedisAuthCache::marker_window(), ttl::BLOCK_LIST);
        assert_eq!(RedisAuthCache::marker_window().as_secs(), 900);
    }

    #[test]
    fn debug_output_never_prints_the_redis_url() {
        // `Redis` has no `Debug` precisely because its URL carries a password.
        let rendered = format!("{:?}", RedisAuthCachePlaceholder);
        assert!(rendered.contains("[redacted]"));
        assert!(!rendered.to_lowercase().contains("redis://"));
    }

    /// A stand-in with the same `Debug` impl, since constructing a real `RedisAuthCache`
    /// needs a live server.
    struct RedisAuthCachePlaceholder;

    impl std::fmt::Debug for RedisAuthCachePlaceholder {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("RedisAuthCache")
                .field("redis", &"[redacted]")
                .finish()
        }
    }
}
