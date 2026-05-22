use crate::shared::constants::{BLOCKLIST_PREFIX, GLOBAL_CACHE_PREFIX, USER_CACHE_PREFIX};
use fred::clients::Pipeline;
use fred::prelude::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{from_str, to_string};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone)]
pub struct RedisService {
    pool: Pool,
}

impl RedisService {
    pub async fn new(url: &str) -> anyhow::Result<Self> {
        let config = Config::from_url(url)?;
        let pool = Builder::from_config(config)
            .set_policy(ReconnectPolicy::new_exponential(0, 100, 30_000, 2))
            .build_pool(10)?;

        pool.init().await?;
        Ok(Self { pool })
    }

    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, Error> {
        let data: Option<String> = self.pool.get(key).await?;
        match data {
            Some(json) => from_str(&json).map(Some).map_err(Error::from),
            None => Ok(None),
        }
    }
    pub async fn set<T: Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &T,
        ex: Option<i64>,
    ) -> Result<(), Error> {
        let serialized = to_string(value)?;
        let expire = ex.map(|e| Expiration::EX(e));
        self.pool
            .set::<(), _, _>(key, serialized, expire, None, false)
            .await?;
        Ok(())
    }
    pub async fn del(&self, key: &str) -> Result<(), Error> {
        self.pool.del::<(), _>(key).await?;
        Ok(())
    }
    pub async fn hset<T: Serialize + Send + Sync>(
        &self,
        key: &str,
        fields: HashMap<String, String>,
    ) -> Result<(), Error> {
        self.pool.hset::<(), _, _>(key, fields).await?;
        Ok(())
    }
    pub async fn hgetall(&self, key: &str) -> Result<HashMap<String, String>, Error> {
        self.pool.hgetall(key).await
    }

    // Helper methods
    pub fn client(&self) -> Pool {
        self.pool.clone()
    }
    pub fn pipeline(&self) -> Pipeline<Client> {
        self.pool.next().pipeline().clone()
    }
    pub async fn exists(&self, key: &str) -> Result<bool, Error> {
        self.pool.exists::<bool, &str>(&key).await
    }
    pub async fn incr_and_expire_if_first(
        &self,
        key: &str,
        ttl_seconds: i64,
    ) -> Result<u64, Error> {
        let script = r#"
            local key = KEYS[1]
            local ttl = tonumber(ARGV[1])

            local count = redis.call('INCR', key)

            if count == 1 then
                redis.call('EXPIRE', key, ttl)
            end

            return count
        "#;

        let count: u64 = self
            .pool
            .eval::<u64, _, _, _>(script, vec![key], vec![ttl_seconds])
            .await?;

        Ok(count)
    }

    // Key Builders
    /// Global keys
    /// Used for non-user-specific caching
    pub fn global_key(name: &str) -> String {
        format!("{}:{}", GLOBAL_CACHE_PREFIX, name)
    }

    /// Example
    ///
    /// ```rust
    /// fn cache_search_results(q: &str, page: i64, limit: i64) -> Result<(), Error> {
    ///    let built_key_suffix = format!("{}:{}:{}", q, page, limit);
    ///    let search_key = search_key(built_key_suffix);
    ///    // ...
    ///    Ok(())
    /// }
    /// ```
    pub fn search_key(suffix: String) -> String {
        format!("{}:SEARCH:{}", GLOBAL_CACHE_PREFIX, suffix)
    }
    /// Used to store the `csrf_token` or state from [oauth2] providers like
    /// Google, Facebook, Apple
    pub fn oauth2_key(state: &str) -> String {
        format!("{}:OAUTH2:{}", GLOBAL_CACHE_PREFIX, state)
    }
    pub fn block_list_key(name: &str, identifier: Uuid) -> String {
        format!("{}:{}:{}", BLOCKLIST_PREFIX, name, identifier)
    }
    /// All user-related cache keys should start with this prefix
    pub fn user_key_prefix(user_id: Uuid) -> String {
        format!("{}:{}:", USER_CACHE_PREFIX, user_id)
    }
    pub fn profile_key(user_id: Uuid) -> String {
        format!("{}:PROFILE:{}", USER_CACHE_PREFIX, user_id)
    }
    pub fn user_providers_key(user_id: Uuid) -> String {
        format!("{}:PROVIDERS:{}", USER_CACHE_PREFIX, user_id)
    }
    pub fn followers_key(user_id: Uuid) -> String {
        format!("{}:FOLLOWERS:{}", USER_CACHE_PREFIX, user_id)
    }
    pub fn following_key(user_id: Uuid) -> String {
        format!("{}:FOLLOWING:{}", USER_CACHE_PREFIX, user_id)
    }
    pub fn broadcasts_key(user_id: Uuid) -> String {
        format!("{}:BROADCASTS:{}", USER_CACHE_PREFIX, user_id)
    }
    pub fn user_session_data_key(user_id: Uuid) -> String {
        format!("{}:SESSION:{}", USER_CACHE_PREFIX, user_id)
    }
    pub fn otp_key(email: &str, otp_type: &str) -> String {
        format!("OTP:{}:{}", otp_type, email)
    }
    pub fn rate_limit_key(prefix: &str, identifier: &str) -> String {
        format!("RT:{}:{}", prefix, identifier)
    }

    /// Invalidates all user-specific keys in the redis cache
    pub async fn invalidate_all_user_keys(&self, user_id: Uuid) -> Result<u64, Error> {
        let pattern = format!("{}*{}*", USER_CACHE_PREFIX, user_id);

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
}

fn _create_tls_config() -> TlsConnector {
    use fred::native_tls::TlsConnector as NativeTlsConnector;
    NativeTlsConnector::builder()
        .use_sni(true)
        .danger_accept_invalid_certs(false)
        .danger_accept_invalid_certs(false)
        .build()
        .expect("Failed to create TLS config")
        .into()
}
