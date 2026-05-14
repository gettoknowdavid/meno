use fred::prelude::*;
pub struct RedisService {
    pub client: Pool,
}
impl RedisService {
    pub async fn new(url: &str) -> anyhow::Result<Self> {
        let config = Config::from_url(url)?;
        let client = Builder::from_config(config)
            .set_policy(ReconnectPolicy::new_exponential(0, 100, 30_000, 2))
            .build_pool(5)?;
        client.init().await?;
        Ok(Self { client })
    }
}
