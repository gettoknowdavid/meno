use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*};

pub fn init_telemetry() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // Default: info for meno_api, warn for noisy deps
        EnvFilter::new("meno_api=info,tower_http=debug,sqlx=warn,fred=warn")
    });

    let fmt_layer = if std::env::var("ENV").as_deref() == Ok("prod") {
        // Production: JSON structured output (consumed by log aggregators)
        fmt::layer()
            .json()
            .with_current_span(true)
            .with_span_list(true)
            .with_target(true)
            .with_thread_ids(true)
            .boxed()
    } else {
        // Development: pretty output with colors
        fmt::layer()
            .pretty()
            .with_target(true)
            .with_file(true)
            .with_line_number(true)
            .boxed()
    };

    Registry::default().with(env_filter).with(fmt_layer).init();
}
