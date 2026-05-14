use fred::prelude::Config;
use fred::prelude::*;
pub struct RedisService {
    pub client: Pool,
}

fn create_tls_config() -> TlsConnector {
    use fred::native_tls::TlsConnector as NativeTlsConnector;

    // or use `TlsConnector::default_native_tls()`
    NativeTlsConnector::builder()
        .use_sni(true)
        .danger_accept_invalid_certs(false)
        .danger_accept_invalid_certs(false)
        .build()
        .expect("Failed to create TLS config")
        .into()
}

impl RedisService {
    pub async fn new(url: &str) -> anyhow::Result<Self> {
        let mut config = Config::from_url(url)?;

        config.tls = Some(create_tls_config().into());

        let client = Builder::from_config(config)
            .with_connection_config(|config| {
                config.connection_timeout = std::time::Duration::from_secs(10);
                config.tcp = TcpConfig {
                    nodelay: Some(true),
                    ..Default::default()
                };
            })
            .set_policy(ReconnectPolicy::new_exponential(0, 100, 30_000, 2))
            .build_pool(5)?;
        client.init().await?;
        Ok(Self { client })
    }
}
