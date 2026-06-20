use crate::shared::services::redis::keys::RedisKey;
use fred::clients::Pipeline;
use fred::prelude::*;
use fred::types::{MultipleKeys, MultipleValues};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{from_str, to_string};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

pub mod coalescing;
pub mod keys;

/// Configuration for Redis connection
#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub url: String,
    pub max_connections: usize,
    pub connection_timeout: Duration,
    pub reconnect_backoff_ms: u32,
    pub reconnect_max_delay_ms: u32,
}
impl RedisConfig {
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
            max_connections: 10,
            connection_timeout: Duration::from_secs(5),
            reconnect_backoff_ms: 100,
            reconnect_max_delay_ms: 30_000,
        }
    }
}

#[derive(Clone)]
pub struct Redis {
    pool: Pool,
}
impl Redis {
    pub async fn new(config: RedisConfig) -> anyhow::Result<Self> {
        let min_delay = config.reconnect_backoff_ms;
        let max_delay = config.reconnect_max_delay_ms;
        let pool = Builder::from_config(Config::from_url(&config.url)?)
            .set_policy(ReconnectPolicy::new_exponential(0, min_delay, max_delay, 2))
            .build_pool(config.max_connections)?;

        pool.init().await?;

        Ok(Self { pool })
    }

    #[must_use]
    pub fn config(&self) -> Config {
        self.pool.client_config()
    }

    pub async fn get<T: DeserializeOwned>(&self, key: &RedisKey) -> Result<Option<T>, Error> {
        let data: Option<String> = self.pool.get(key.as_ref()).await?;
        match data {
            Some(json) => from_str(&json).map(Some).map_err(Error::from),
            None => Ok(None),
        }
    }
    pub async fn set<T: Serialize + Send + Sync>(
        &self,
        key: &RedisKey,
        value: &T,
        ex: Option<i64>,
    ) -> Result<(), Error> {
        let serialized = to_string(value)?;
        let expire = ex.map( Expiration::EX);
        self.pool
            .set::<(), _, _>(key.as_ref(), serialized, expire, None, false)
            .await?;
        Ok(())
    }
    pub async fn del(&self, key: &RedisKey) -> Result<i64, Error> {
        self.pool.del(key.as_ref()).await
    }
    pub async fn hset<T: Serialize + Send + Sync>(
        &self,
        key: &RedisKey,
        fields: HashMap<String, String>,
    ) -> Result<(), Error> {
        self.pool.hset::<(), _, _>(key.as_ref(), fields).await?;
        Ok(())
    }
    pub async fn hgetall(&self, key: &RedisKey) -> Result<HashMap<String, String>, Error> {
        self.pool.hgetall(key.as_ref()).await
    }

    // Helper methods
    #[must_use]
    pub fn client(&self) -> Pool {
        self.pool.clone()
    }

    #[must_use]
    pub fn pipeline(&self) -> Pipeline<Client> {
        self.pool.next().pipeline().clone()
    }
    pub async fn exists(&self, key: &RedisKey) -> Result<bool, Error> {
        self.pool.exists::<bool, &str>(key.as_ref()).await
    }
    pub async fn incr_and_expire_if_first(
        &self,
        key: &RedisKey,
        ttl_seconds: i64,
    ) -> Result<u64, Error> {
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
            .pool
            .eval::<u64, _, _, _>(script, vec![key.as_ref()], vec![ttl_seconds])
            .await?;

        Ok(count)
    }
    pub async fn get_i64(&self, key: &RedisKey) -> Result<i64, Error> {
        let val: Option<i64> = self.pool.get(key.as_ref()).await?;
        Ok(val.unwrap_or(0))
    }
    pub async fn incr(&self, key: &RedisKey) -> Result<i64, Error> {
        self.pool.incr(key.as_ref()).await
    }
    pub async fn decr(&self, key: &RedisKey) -> Result<i64, Error> {
        self.pool.decr(key.as_ref()).await
    }
    pub async fn set_ex(&self, key: &RedisKey, value: &str, ttl_secs: u64) -> Result<(), Error> {
        let ttl = Some(Expiration::EX(ttl_secs.cast_signed()));
        self.pool
            .set::<(), _, _>(key.as_ref(), value, ttl, None, false)
            .await
    }
    pub async fn expire(&self, key: &RedisKey, ttl_secs: i64) -> Result<(), Error> {
        self.pool
            .expire::<(), _>(key.as_ref(), ttl_secs, None)
            .await
    }
    pub async fn lpush(&self, key: &RedisKey, value: &str) -> Result<(), Error> {
        self.pool.lpush::<(), _, _>(key.as_ref(), value).await
    }
    pub async fn ltrim(&self, key: &RedisKey, start: i64, stop: i64) -> Result<(), Error> {
        self.pool.ltrim::<(), _>(key.as_ref(), start, stop).await
    }
    pub async fn eval<T, K, V>(&self, script: &str, keys: K, args: V) -> Result<T, Error>
    where
        T: FromValue,
        K: Into<MultipleKeys> + Send,
        V: TryInto<MultipleValues> + Send,
        V::Error: Into<Error> + Send,
    {
        self.pool.eval(script, keys, args).await
    }

    pub async fn sadd<R, V>(&self, key: &RedisKey, members: V) -> Result<R, Error>
    where
        R: FromValue,
        V: TryInto<MultipleValues> + Send,
        V::Error: Into<Error> + Send,
    {
        self.pool.sadd(key.as_ref(), members).await
    }

    pub async fn srem<R, V>(&self, key: &RedisKey, members: V) -> Result<R, Error>
    where
        R: FromValue,
        V: TryInto<MultipleValues> + Send,
        V::Error: Into<Error> + Send,
    {
        self.pool.srem(key.as_ref(), members).await
    }

    pub async fn scard<R>(&self, key: &RedisKey) -> Result<R, Error>
    where
        R: FromValue,
    {
        self.pool.scard(key.as_ref()).await
    }

    pub async fn sismember<R, V>(&self, key: &RedisKey, member: V) -> Result<R, Error>
    where
        R: FromValue,
        V: TryInto<Value> + Send,
        V::Error: Into<Error> + Send,
    {
        self.pool.sismember(key.as_ref(), member).await
    }

    pub async fn smembers<R>(&self, key: &RedisKey) -> Result<R, Error>
    where
        R: FromValue,
    {
        self.pool.smembers(key.as_ref()).await
    }

    /// Invalidates all user-specific keys in the redis cache
    pub async fn invalidate_all_user_keys(&self, user_id: Uuid) -> Result<u64, Error> {
        let pattern = format!("u:{user_id}:*");

        let mut cursor = "0".to_string();
        let mut total_deleted: u64 = 0;

        loop {
            let (new_cursor, keys): (String, Vec<Key>) = self
                .pool
                .scan_page(cursor, &pattern, Some(300), None)
                .await?;

            if !keys.is_empty() {
                self.pool.unlink::<(), _>(keys.clone()).await?;
                total_deleted += keys.len() as u64;
            }

            cursor = new_cursor;
            if cursor == "0" {
                break;
            }

            tokio::task::yield_now().await;
        }

        if total_deleted > 0 {
            tracing::info!(
                user_id = %user_id,
                deleted = total_deleted,
                "User cache invalidated"
            );
        }

        Ok(total_deleted)
    }

    pub async fn delete_by_pattern(&self, pattern: &str) -> Result<u64, Error> {
        let mut cursor = "0".to_string();
        let mut deleted = 0u64;

        loop {
            let (new_cursor, keys): (String, Vec<Key>) = self
                .pool
                .scan_page(cursor.clone(), pattern, Some(200), None)
                .await?;

            if !keys.is_empty() {
                // `unlink` is async-delete (non-blocking on Redis server side)
                self.pool.unlink::<(), _>(keys.clone()).await?;
                deleted += keys.len() as u64;
            }
            // ↑ The second `deleted += keys.len()` at the bottom is removed

            cursor = new_cursor;
            if cursor == "0" {
                break;
            }

            // Yield to allow other Tokio tasks to run between scan pages.
            // Important when the keyspace is large (millions of keys).
            tokio::task::yield_now().await;
        }

        if deleted > 0 {
            tracing::debug!(deleted, pattern, "cache keys evicted");
        }

        Ok(deleted)
    }

    pub async fn publish(&self, channel: &str, message: String) -> Result<(), Error> {
        self.pipeline().publish::<(), _, _>(channel, message).await
    }
}
