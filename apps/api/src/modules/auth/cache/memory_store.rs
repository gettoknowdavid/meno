//! An in-memory [`AuthCache`] for tests.
//!
//! The same trade as [`InMemoryAuthRepo`](super::super::repository::InMemoryAuthRepo):
//! real state the token tests can set up and read back, rather than a mock whose
//! expectations have to be restated per test.
//!
//! One deliberate difference from Redis: entries here never expire, because a test that
//! had to wait fifteen minutes to observe an expiry would not be written. The
//! *validation* that matters — refusing a non-positive TTL — is asserted against the
//! Redis adapter instead, in [`super::tests`].

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use async_trait::async_trait;
use meno_core::Error as MenoError;
use uuid::Uuid;

use super::AuthCache;
use crate::modules::auth::error;

/// A blocklist backed by `HashMap`s.
#[derive(Debug, Default)]
pub struct InMemoryAuthCache {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    /// Blocked token ids, with the instant they were written.
    tokens: HashMap<Uuid, Instant>,
    /// User id to when the logout-all marker was written, in seconds since the epoch.
    ///
    /// Stored in the token's own `iat` unit rather than as an [`Instant`], so the
    /// comparison in `is_user_tokens_blocked` does not convert between two clocks.
    users: HashMap<Uuid, i64>,
}

impl InMemoryAuthCache {
    /// An empty blocklist.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `user_id` has a logout-all marker at all.
    ///
    /// A convenience for assertions; the service never asks this question, because a
    /// user with no marker is simply "not blocked".
    #[must_use]
    pub fn is_user_blocked(&self, user_id: Uuid) -> bool {
        self.lock().users.contains_key(&user_id)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // `unwrap_or_else(|e| e.into_inner())` rather than `unwrap`: a poisoned lock
        // means another test panicked while holding it, and the data is still readable.
        // §9.1 forbids the panic.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Refuse a non-positive TTL before anything is written (§9.4).
fn refuse_non_positive(operation: &'static str, ttl_secs: i64) -> Option<MenoError> {
    if ttl_secs > 0 {
        return None;
    }
    Some(error::internal(
        operation,
        format!("refused a {ttl_secs}-second blocklist TTL; it must be positive"),
    ))
}

#[async_trait]
impl AuthCache for InMemoryAuthCache {
    async fn block_access_token(&self, jti: Uuid, ttl_secs: i64) -> Result<(), MenoError> {
        if let Some(problem) = refuse_non_positive("block_access_token", ttl_secs) {
            return Err(problem);
        }
        self.lock().tokens.insert(jti, Instant::now());
        Ok(())
    }

    async fn is_token_blocked(&self, jti: Uuid) -> Result<bool, MenoError> {
        Ok(self.lock().tokens.contains_key(&jti))
    }

    async fn block_all_user_tokens(
        &self,
        user_id: Uuid,
        window_secs: i64,
    ) -> Result<(), MenoError> {
        if let Some(problem) = refuse_non_positive("block_all_user_tokens", window_secs) {
            return Err(problem);
        }
        self.lock()
            .users
            .insert(user_id, time::OffsetDateTime::now_utc().unix_timestamp());
        Ok(())
    }

    async fn is_user_tokens_blocked(
        &self,
        user_id: Uuid,
        issued_at: i64,
    ) -> Result<bool, MenoError> {
        let Some(marked_at) = self.lock().users.get(&user_id).copied() else {
            return Ok(false);
        };

        // Both sides are seconds since the epoch, so this is one comparison rather than
        // a conversion -- and a conversion is where a units bug hides.
        let issued_at = issued_at.max(0);
        Ok(issued_at <= marked_at)
    }
}
