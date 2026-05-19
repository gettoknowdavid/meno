use fred::clients::Pipeline;
use fred::prelude::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{from_str, to_string};
use std::collections::HashMap;

#[derive(Clone)]
pub struct RedisService {
    pool: Pool,
}

impl RedisService {
    pub async fn new(url: &str) -> anyhow::Result<Self> {
        let mut config = Config::from_url(url)?;
        config.tls = Some(create_tls_config().into());
        let pool = Builder::from_config(config)
            .with_connection_config(|config| {
                config.connection_timeout = std::time::Duration::from_secs(10);
                config.tcp = TcpConfig {
                    nodelay: Some(true),
                    ..Default::default()
                };
            })
            .set_policy(ReconnectPolicy::new_exponential(0, 100, 30_000, 2))
            .build_pool(5)?;

        pool.init().await?;
        Ok(Self { pool })
    }
    pub fn client(&self) -> Pool {
        self.pool.clone()
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
    pub fn pipeline(&self) -> Pipeline<Client> {
        self.pool.next().pipeline().clone()
    }
}

fn create_tls_config() -> TlsConnector {
    use fred::native_tls::TlsConnector as NativeTlsConnector;
    NativeTlsConnector::builder()
        .use_sni(true)
        .danger_accept_invalid_certs(false)
        .danger_accept_invalid_certs(false)
        .build()
        .expect("Failed to create TLS config")
        .into()
}
