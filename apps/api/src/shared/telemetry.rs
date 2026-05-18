use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*};
pub fn init_telemetry() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "meno_api=debug,tower_http=info".into());

    let formatting_layer = fmt::layer().with_thread_ids(true).with_target(false).json();

    Registry::default()
        .with(env_filter)
        .with(formatting_layer)
        .init();
}
