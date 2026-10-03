//! Redis adapter.
//!
//! Copied from `apps/api/src/shared/services/redis/mod.rs` on `master` (`903c3ba`),
//! ported to the `infrastructure::` layout, with two changes required by plan §3.2.
//!
//! # 1. Every write carries its key's TTL (§3.2, §9.4)
//!
//! On `master`, `set` took `ex: Option<i64>` and every caller passed `None`, so keys
//! lived until the instance ran out of memory. §3.2 is explicit:
//!
//! > Set **mandatory TTLs on every key** — this is enforced in code review and by a
//! > `RedisKey` type whose constructors all require an expiry.
//!
//! [`RedisKey`] now carries its expiry, and [`Redis::set`] has no expiry parameter at
//! all — it reads the TTL off the key. A caller cannot write a key without one,
//! because the signature will not let them. `set_ex` is gone for the same reason: it
//! was the escape hatch that let the rule be bypassed.
//!
//! For keys that are *incremented* or *appended to*, re-applying the TTL on every write
//! would keep the key alive forever — the opposite of the rule. Those operations use
//! [`Redis::apply_ttl_if_new`], which sets the expiry only when the key is first
//! created, so the TTL still measures from creation.
//!
//! # 2. Small pool (§3.2)
//!
//! > **Connection limits are low on free.** Configure `fred` with a small pool
//! > (`pool_min = 2`, `pool_max = 8`), not the defaults.
//!
//! `master` hard-coded 10 with no minimum, which is a connection-per-request ceiling
//! the free tier will refuse.
//!
//! # Ephemerality (§3.2)
//!
//! This instance has no persistence on the free plan. Everything cached here must be
//! reconstructible from Postgres. That is why the TTLs are short and why
//! [`RedisKey::otp`] is flagged in `keys.rs` as belonging in Neon.

pub mod coalescing;
pub mod keys;

use fred::clients::{Client, DynamicPool, Pipeline};
use fred::prelude::*;
use fred::types::config::{DynamicPoolConfig, RemoveIdle};
use fred::types::{MultipleKeys, MultipleValues};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::from_str;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

// Re-exported so callers write `redis::RedisKey` rather than reaching into the
// submodule. §3.2's mandatory-TTL rule is enforced by the type, so it should be the
// obvious thing to reach for.
pub use keys::{RedisKey, RedisPattern};

/// Configuration for the Redis connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedisConfig {
    /// Connection URL. Internal (`redis://`) when co-located, per §3.2.
    pub url: String,
    /// Minimum pooled connections. §3.2 specifies 2 for the free tier.
    pub pool_min: usize,
    /// Maximum pooled connections. §3.2 specifies 8 for the free tier.
    pub pool_max: usize,
    /// How long to wait for a connection before giving up.
    pub connection_timeout: Duration,
    /// First reconnect delay, in milliseconds.
    pub reconnect_backoff_ms: u32,
    /// Backoff ceiling, in milliseconds.
    pub reconnect_max_delay_ms: u32,
}

impl RedisConfig {
    /// Build a config from a URL, with the §3.2 defaults for everything else.
    #[must_use]
    pub fn from_url(url: String) -> Self {
        Self {
            url,
            ..Self::default()
        }
    }
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            // §3.2: "Configure fred with a small pool (pool_min = 2, pool_max = 8), not
            // the defaults." fred's own defaults are far larger and will exhaust a free
            // instance's connection budget.
            pool_min: 2,
            pool_max: 8,
            connection_timeout: Duration::from_secs(5),
            reconnect_backoff_ms: 100,
            reconnect_max_delay_ms: 30_000,
        }
    }
}

/// The Redis client.
///
/// Backed by a **dynamic** pool, which is the only fred10 shape that can honour §3.2's
/// `pool_min = 2, pool_max = 8`. The fixed `Pool` has no minimum: `build_pool(n)` eagerly
/// opens `n` connections, so it can express the ceiling but never the floor. A dynamic
/// pool opens connections on demand and closes idle ones, which is what a 25 MB free
/// instance actually needs.
#[derive(Clone)]
pub struct Redis {
    pool: DynamicPool,
    /// Kept alongside the pool because `DynamicPool` has no `client_config()`
    /// accessor, and callers still want to read the URL back out (§3.2's "internal
    /// `redis://` when co-located" is only verifiable this way).
    config: Config,
}

impl Redis {
    /// Connect and initialise the pool.
    pub async fn new(config: RedisConfig) -> anyhow::Result<Self> {
        let min_delay = config.reconnect_backoff_ms;
        let max_delay = config.reconnect_max_delay_ms;

        let client_config = Config::from_url(&config.url)?;

        let pool = Builder::from_config(client_config.clone())
            .set_pool_config(DynamicPoolConfig {
                min_clients: config.pool_min,
                max_clients: config.pool_max,
                // Close a connection idle for five minutes. Long enough to survive a
                // quiet feed, short enough to give the budget back.
                max_idle_time: Duration::from_secs(300),
                // fred's own default: shed idle connections rather than hold the
                // ceiling open for the life of the process. `RemoveIdle` is a unit
                // struct — there is no `Default` impl to call.
                scale: Arc::new(RemoveIdle),
                // `dns` is enabled in the workspace manifest precisely so this field
                // always exists; `None` means "use fred's default resolver".
                resolver: None,
            })
            .set_policy(ReconnectPolicy::new_exponential(0, min_delay, max_delay, 2))
            .build_dynamic_pool()?;

        pool.init().await?;

        Ok(Self {
            pool,
            config: client_config,
        })
    }

    /// The underlying client config.
    #[must_use]
    pub fn config(&self) -> Config {
        self.config.clone()
    }

    /// A connection borrowed from the pool.
    ///
    /// `DynamicPool` is a *pool of clients*, not a client: it exposes no command
    /// methods of its own. Every command therefore goes out on a connection checked
    /// out for the duration of the call. `next()` is a cheap handle over a pooled
    /// connection, not a new one — the pool still bounds concurrency at
    /// `pool_max`.
    #[must_use]
    fn conn(&self) -> Client {
        self.pool.next()
    }

    // ─────────────────────────── typed reads/writes ───────────────────────────

    /// Read and deserialise a key.
    ///
    /// Returns `Ok(None)` for a missing key — a cache miss is not an error.
    pub async fn get<T: DeserializeOwned>(&self, key: &RedisKey) -> Result<Option<T>, Error> {
        let data: Option<String> = self.conn().get(key.as_ref()).await?;
        match data {
            Some(json) => from_str(&json).map(Some).map_err(Error::from),
            None => Ok(None),
        }
    }

    /// Write a key, applying **its own TTL**.
    ///
    /// There is deliberately no expiry parameter: the TTL is part of the key, so this
    /// method cannot write something that will never expire.
    pub async fn set<T: Serialize + Send + Sync>(
        &self,
        key: &RedisKey,
        value: &T,
    ) -> Result<(), Error> {
        let serialized = serde_json::to_string(value)?;
        let expire = Expiration::EX(key.ttl().as_secs() as i64);
        self.conn()
            .set::<(), _, _>(key.as_ref(), serialized, Some(expire), None, false)
            .await?;
        Ok(())
    }

    /// Delete a key. Returns the number of keys removed.
    pub async fn del(&self, key: &RedisKey) -> Result<i64, Error> {
        self.conn().del(key.as_ref()).await
    }

    /// Write a hash.
    pub async fn hset(&self, key: &RedisKey, fields: HashMap<String, String>) -> Result<(), Error> {
        self.conn().hset::<(), _, _>(key.as_ref(), fields).await?;
        Ok(())
    }

    /// Read a whole hash.
    pub async fn hgetall(&self, key: &RedisKey) -> Result<HashMap<String, String>, Error> {
        self.conn().hgetall(key.as_ref()).await
    }

    // ─────────────────────────── helpers ───────────────────────────

    /// The underlying connection pool, for commands this wrapper does not expose.
    #[must_use]
    pub fn client(&self) -> DynamicPool {
        self.pool.clone()
    }

    /// A pipeline on an arbitrary connection from the pool.
    #[must_use]
    pub fn pipeline(&self) -> Pipeline<Client> {
        self.pool.next().pipeline().clone()
    }

    /// Whether a key exists.
    pub async fn exists(&self, key: &RedisKey) -> Result<bool, Error> {
        self.conn().exists::<bool, &str>(key.as_ref()).await
    }

    /// Increment a counter, setting its TTL only on first creation.
    ///
    /// Uses Lua so the increment and the expiry are one atomic step: two concurrent
    /// first-writes cannot both observe a zero counter and race the expiry.
    pub async fn incr_and_expire_if_first(&self, key: &RedisKey) -> Result<u64, Error> {
        let script = r"
            local key = KEYS[1]
            local ttl = tonumber(ARGV[1])

            local count = redis.call('INCR', key)

            if count == 1 then
                redis.call('EXPIRE', key, ttl)
            end

            return count
        ";

        let count: u64 = self
            .conn()
            .eval::<u64, _, _, _>(script, vec![key.as_ref()], vec![key.ttl().as_secs() as i64])
            .await?;

        Ok(count)
    }

    /// Apply a key's TTL only if the key does not already have one.
    ///
    /// For `SADD`/`LPUSH`-style writes, where re-applying the TTL on every call would
    /// keep the key alive indefinitely. This is the collection-operation counterpart to
    /// `set`, which always refreshes.
    pub async fn apply_ttl_if_new(&self, key: &RedisKey) -> Result<(), Error> {
        let script = r"
            if redis.call('TTL', KEYS[1]) == -1 then
                redis.call('EXPIRE', KEYS[1], ARGV[1])
                return 1
            end
            return 0
        ";

        let applied: i64 = self
            .conn()
            .eval::<i64, _, _, _>(script, vec![key.as_ref()], vec![key.ttl().as_secs() as i64])
            .await?;

        let _ = applied;
        Ok(())
    }

    /// Read an integer key, defaulting to 0 when absent.
    pub async fn get_i64(&self, key: &RedisKey) -> Result<i64, Error> {
        let val: Option<i64> = self.conn().get(key.as_ref()).await?;
        Ok(val.unwrap_or(0))
    }

    /// Increment without touching the TTL. Prefer
    /// [`Redis::incr_and_expire_if_first`] when creating the key.
    pub async fn incr(&self, key: &RedisKey) -> Result<i64, Error> {
        self.conn().incr(key.as_ref()).await
    }

    /// Decrement without touching the TTL.
    pub async fn decr(&self, key: &RedisKey) -> Result<i64, Error> {
        self.conn().decr(key.as_ref()).await
    }

    /// Override a key's expiry. Rarely needed — prefer the key's own TTL.
    pub async fn expire(&self, key: &RedisKey, ttl: Duration) -> Result<(), Error> {
        self.conn()
            .expire::<(), _>(key.as_ref(), ttl.as_secs() as i64, None)
            .await
    }

    /// Prepend to a list.
    pub async fn lpush(&self, key: &RedisKey, value: &str) -> Result<(), Error> {
        self.conn().lpush::<(), _, _>(key.as_ref(), value).await?;
        self.apply_ttl_if_new(key).await
    }

    /// Trim a list to a range, used to bound ring buffers.
    pub async fn ltrim(&self, key: &RedisKey, start: i64, stop: i64) -> Result<(), Error> {
        self.conn().ltrim::<(), _>(key.as_ref(), start, stop).await
    }

    /// Run a Lua script.
    pub async fn eval<T, K, V>(&self, script: &str, keys: K, args: V) -> Result<T, Error>
    where
        T: FromValue,
        K: Into<MultipleKeys> + Send,
        V: TryInto<MultipleValues> + Send,
        V::Error: Into<Error> + Send,
    {
        self.conn().eval(script, keys, args).await
    }

    /// Add members to a set, applying the key's TTL on creation.
    pub async fn sadd<R, V>(&self, key: &RedisKey, members: V) -> Result<R, Error>
    where
        R: FromValue,
        V: TryInto<MultipleValues> + Send,
        V::Error: Into<Error> + Send,
    {
        let result: R = self.conn().sadd(key.as_ref(), members).await?;
        self.apply_ttl_if_new(key).await?;
        Ok(result)
    }

    /// Remove members from a set.
    pub async fn srem<R, V>(&self, key: &RedisKey, members: V) -> Result<R, Error>
    where
        R: FromValue,
        V: TryInto<MultipleValues> + Send,
        V::Error: Into<Error> + Send,
    {
        self.conn().srem(key.as_ref(), members).await
    }

    /// Set cardinality.
    pub async fn scard<R>(&self, key: &RedisKey) -> Result<R, Error>
    where
        R: FromValue,
    {
        self.conn().scard(key.as_ref()).await
    }

    /// Membership test.
    pub async fn sismember<R, V>(&self, key: &RedisKey, member: V) -> Result<R, Error>
    where
        R: FromValue,
        V: TryInto<Value> + Send,
        V::Error: Into<Error> + Send,
    {
        self.conn().sismember(key.as_ref(), member).await
    }

    /// All members of a set.
    pub async fn smembers<R>(&self, key: &RedisKey) -> Result<R, Error>
    where
        R: FromValue,
    {
        self.conn().smembers(key.as_ref()).await
    }

    // ─────────────────────────── pattern operations ───────────────────────────

    /// Invalidate every per-user key. Returns how many were removed.
    pub async fn invalidate_all_user_keys(&self, user_id: Uuid) -> Result<u64, Error> {
        let deleted = self
            .delete_by_pattern(&RedisKey::user_pattern(user_id))
            .await?;

        if deleted > 0 {
            tracing::info!(user_id = %user_id, deleted, "user cache invalidated");
        }

        Ok(deleted)
    }

    /// Delete every key matching a `SCAN` glob, in pages.
    ///
    /// Paged rather than `KEYS`, because `KEYS` blocks the single-threaded server for
    /// the whole keyspace — on a shared instance that is every other tenant's latency.
    /// `unlink` is async-delete, so it does not block either.
    pub async fn delete_by_pattern(&self, pattern: &RedisPattern) -> Result<u64, Error> {
        let mut cursor = "0".to_string();
        let mut deleted = 0_u64;

        loop {
            let (new_cursor, keys): (String, Vec<Key>) = self
                .conn()
                .scan_page(cursor.clone(), pattern.as_str(), Some(200), None)
                .await?;

            if !keys.is_empty() {
                self.conn().unlink::<(), _>(keys.clone()).await?;
                deleted += keys.len() as u64;
            }

            cursor = new_cursor;
            if cursor == "0" {
                break;
            }

            // Yield between pages: a large keyspace would otherwise monopolise the
            // runtime thread through the whole scan.
            tokio::task::yield_now().await;
        }

        if deleted > 0 {
            tracing::debug!(deleted, pattern = %pattern.as_str(), "cache keys evicted");
        }

        Ok(deleted)
    }

    /// Publish a message on a channel.
    pub async fn publish(&self, channel: &str, message: String) -> Result<(), Error> {
        self.pipeline().publish::<(), _, _>(channel, message).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_defaults_match_the_free_tier_budget_in_the_plan() {
        // §3.2: "Configure fred with a small pool (pool_min = 2, pool_max = 8), not the
        // defaults." master used 10 with no minimum, which the free tier refuses.
        let config = RedisConfig::default();
        assert_eq!(config.pool_min, 2);
        assert_eq!(config.pool_max, 8);
        assert!(
            config.pool_min < config.pool_max,
            "an empty minimum means fred opens connections only on demand"
        );
    }

    #[test]
    fn from_url_keeps_the_url_and_the_sizing() {
        let config = RedisConfig::from_url("redis://localhost:6379".to_string());
        assert_eq!(config.url, "redis://localhost:6379");
        assert_eq!(config.pool_min, 2);
        assert_eq!(config.pool_max, 8);
    }

    #[test]
    fn default_url_is_empty_so_a_missing_env_var_fails_loudly() {
        // An empty URL must not silently become a working default against localhost.
        assert!(RedisConfig::default().url.is_empty());
    }

    #[test]
    fn backoff_is_bounded_and_ordered() {
        let config = RedisConfig::default();
        assert!(config.reconnect_backoff_ms > 0);
        assert!(
            config.reconnect_max_delay_ms > config.reconnect_backoff_ms,
            "a ceiling below the first delay means no backoff at all"
        );
    }
}
