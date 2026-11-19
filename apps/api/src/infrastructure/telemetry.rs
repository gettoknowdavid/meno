//! Tracing, logging and OTLP export (§4.8).
//!
//! Ported from `apps/api/src/shared/telemetry.rs` on `master` (`903c3ba`), with the log
//! filter read from the validated [`Config`] rather than re-read from the environment.
//!
//! # What this module installs
//!
//! One `tracing_subscriber` registry, three layers:
//!
//! | Layer | Destination | Always on? |
//! | --- | --- | --- |
//! | `fmt` | stdout — pretty in development, JSON in a deployed environment | Yes |
//! | `tracing_opentelemetry` | OTLP *traces* | Only when `OTEL_EXPORTER_OTLP_ENDPOINT` is set |
//! | `OpenTelemetryTracingBridge` | OTLP *logs* | Same switch |
//!
//! # Why stdout stays on when OTLP is on
//!
//! Two independent readers of the same log line, deliberately:
//!
//! - **stdout** is what a platform log drain reads (Render, Fly, `docker logs`). It is
//!   the fallback that works when the collector is down, and it needs no configuration.
//! - **OTLP** is what puts the line in the same backend as the trace it belongs to, with
//!   the trace id attached so a log search leads to the span. That is the correlation
//!   stdout cannot do on its own.
//!
//! # Failure policy
//!
//! Observability must not be a startup dependency. A configured endpoint that cannot be
//! reached leaves the process running with stdout only and logs one `error!` saying so —
//! the alternative (refusing to boot because a collector is down) turns a debugging
//! aid into an outage.
//!
//! The exporter is *built* eagerly, though, so a malformed endpoint is reported at boot
//! instead of on the first span.
//!
//! # JSON in deployed environments
//!
//! Because that is what a log drain parses. Pretty output is for a human at a terminal,
//! and the choice follows `Config::env`, not a second read of `ENV`.

use opentelemetry::KeyValue;
use opentelemetry::trace::{TraceContextExt as _, TracerProvider as _};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{LogExporter, SpanExporter, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::{
    BatchConfigBuilder, BatchSpanProcessor, Sampler, SdkTracerProvider,
};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::{Config, OtelSettings};

/// Keeps the OpenTelemetry providers alive, and flushes them on the way out.
///
/// Held by [`AppContext`](crate::bootstrap::AppContext) so it lives exactly as long as
/// the process's own wiring: dropping it at the end of `main` is what turns the last
/// batch of buffered spans and log records into an export instead of losing them. The
/// batch processors hold spans for up to [`BATCH_DELAY`] and a hard kill loses whatever
/// is still buffered, which is the expected OTLP behaviour rather than a bug.
#[must_use = "dropping the guard immediately shuts the exporters down"]
pub struct TelemetryGuard {
    traces: Option<SdkTracerProvider>,
    logs: Option<SdkLoggerProvider>,
}

impl TelemetryGuard {
    /// Push everything buffered to the collector now.
    ///
    /// A graceful shutdown of the process is the one moment it is worth waiting for:
    /// spans describing the shutdown itself are otherwise dropped. Errors are logged
    /// and swallowed — there is nothing useful a shutting-down process can do about a
    /// collector that has gone away.
    pub fn force_flush(&self) {
        if let Some(traces) = &self.traces
            && let Err(error) = traces.force_flush()
        {
            tracing::error!(%error, "could not flush traces before shutdown");
        }

        if let Some(logs) = &self.logs
            && let Err(error) = logs.force_flush()
        {
            tracing::error!(%error, "could not flush logs before shutdown");
        }
    }

    /// Whether traces reached a provider (i.e. OTLP is configured and usable).
    #[must_use]
    pub fn traces_enabled(&self) -> bool {
        self.traces.is_some()
    }

    /// Whether logs reached a provider.
    #[must_use]
    pub fn logs_enabled(&self) -> bool {
        self.logs.is_some()
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        // Traces first: the log bridge attaches the current trace context to each
        // record, so shutting the tracer down first means the final flush cannot be
        // preceded by a tracer fault that would discard the log batch too.
        if let Some(traces) = self.traces.take()
            && let Err(error) = traces.shutdown()
        {
            tracing::error!(%error, "the tracer provider did not shut down cleanly");
        }

        if let Some(logs) = self.logs.take()
            && let Err(error) = logs.shutdown()
        {
            tracing::error!(%error, "the logger provider did not shut down cleanly");
        }
    }
}

/// Install the global tracing subscriber.
///
/// Returns the guard to hold for the process's lifetime when OTLP export is configured,
/// and `None` when it is not — a development run with no collector gets the stdout
/// subscriber and nothing else.
///
/// # Panics
///
/// Panics if a subscriber is already installed, which happens when a test binary calls
/// it twice. Nothing in this crate does — the binaries call it once at startup — and a
/// double install is a programming error worth failing loudly on rather than
/// silently ignoring.
pub fn init(config: &Config) -> Option<TelemetryGuard> {
    let filter = EnvFilter::try_new(&config.log_filter).unwrap_or_else(|e| {
        // A malformed `RUST_LOG` must not stop the process. Fall back to something
        // quiet and say so, rather than booting with no logs and no explanation.
        // This runs before the subscriber exists, so it cannot be a `tracing` event.
        filter_fallback_note(&config.log_filter, &e);
        EnvFilter::new(DEFAULT_FILTER)
    });

    let mut failures: Vec<String> = Vec::new();

    let traces = config.otel.as_ref().and_then(|otel| {
        tracer_provider(otel, config)
            .map_err(|error| failures.push(format!("traces: {error}")))
            .ok()
    });

    let logs = config.otel.as_ref().and_then(|otel| {
        logger_provider(otel, config)
            .map_err(|error| failures.push(format!("logs: {error}")))
            .ok()
    });

    let trace_layer = traces.as_ref().map(|provider| {
        let tracer = provider.tracer(OTEL_SCOPE);
        tracing_opentelemetry::layer().with_tracer(tracer)
    });
    let log_layer = logs.as_ref().map(OpenTelemetryTracingBridge::new);

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(trace_layer)
        .with(log_layer);

    if config.env.is_deployed() {
        registry
            .with(
                fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(true)
                    .with_target(true),
            )
            .init();
    } else {
        registry
            .with(
                fmt::layer()
                    .pretty()
                    .with_target(true)
                    .with_file(true)
                    .with_line_number(true),
            )
            .init();
    }

    for failure in failures {
        tracing::error!(
            detail = %failure,
            "OpenTelemetry export is configured but could not start; \
             this process has stdout logs only"
        );
    }

    if let Some(otel) = &config.otel {
        tracing::info!(
            endpoint = %otel.endpoint,
            service = %otel.service_name,
            sample_ratio = otel.sample_ratio,
            traces = traces.is_some(),
            logs = logs.is_some(),
            "OpenTelemetry export configured"
        );
    }

    (traces.is_some() || logs.is_some()).then_some(TelemetryGuard { traces, logs })
}

/// The trace id of the span the current task is inside, for log correlation.
///
/// Returns `None` when no OTLP layer is installed (the OpenTelemetry context is then
/// invalid) or when the event was emitted outside any span. The middleware that logs a
/// request puts this field on its line, which is what lets a Loki query jump to the
/// matching trace in Tempo.
#[must_use]
pub fn current_trace_id() -> Option<String> {
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;

    let context = tracing::Span::current().context();
    // `span()` is `TraceContextExt`, imported above; without it the call resolves to
    // `Context`'s private field of the same name.
    let span_context = context.span().span_context().clone();

    span_context
        .is_valid()
        .then(|| span_context.trace_id().to_string())
}

/// The instrumentation scope name attached to exported spans and log records.
const OTEL_SCOPE: &str = "meno-api";

/// How long the batch processor may hold a span before exporting it.
///
/// Kept well below the SDK's 5-second default so a crash loses at most a couple of
/// seconds of traces: on a free tier the process is restarted without warning.
const BATCH_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

/// The most spans one export may carry.
const MAX_EXPORT_BATCH_SIZE: usize = 512;

/// Build the OTLP span pipeline. `Err` carries a message safe to log.
fn tracer_provider(otel: &OtelSettings, config: &Config) -> Result<SdkTracerProvider, String> {
    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(otel.endpoint.clone())
        .build()
        .map_err(|error| error.to_string())?;

    // `ParentBased` so an incoming sampled decision is honoured: without it, a service
    // that samples at 10% would break a trace that a gateway had already sampled.
    let sampler = if otel.sample_ratio >= 1.0 {
        Sampler::AlwaysOn
    } else {
        Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(otel.sample_ratio)))
    };

    // The batch processor is built explicitly rather than through
    // `with_batch_exporter`, because the provider builder offers no way to pass a
    // `BatchConfig` and the default 5-second delay is too long for a free-tier
    // process that can be restarted at any moment.
    let processor = BatchSpanProcessor::builder(exporter)
        .with_batch_config(
            BatchConfigBuilder::default()
                .with_scheduled_delay(BATCH_DELAY)
                .with_max_export_batch_size(MAX_EXPORT_BATCH_SIZE)
                .build(),
        )
        .build();

    let provider = SdkTracerProvider::builder()
        .with_resource(resource(otel, config))
        .with_sampler(sampler)
        .with_span_processor(processor)
        .build();

    Ok(provider)
}

/// Build the OTLP log pipeline. `Err` carries a message safe to log.
fn logger_provider(otel: &OtelSettings, config: &Config) -> Result<SdkLoggerProvider, String> {
    let exporter = LogExporter::builder()
        .with_tonic()
        .with_endpoint(otel.endpoint.clone())
        .build()
        .map_err(|error| error.to_string())?;

    Ok(SdkLoggerProvider::builder()
        .with_resource(resource(otel, config))
        .with_batch_exporter(exporter)
        .build())
}

/// The resource attributes every span and log record carries.
///
/// `service.name` is what a dashboard groups by, and `deployment.environment` is what
/// keeps a staging trace out of a production chart when both write to one backend.
fn resource(otel: &OtelSettings, config: &Config) -> Resource {
    Resource::builder()
        .with_service_name(otel.service_name.clone())
        .with_attribute(KeyValue::new(
            "deployment.environment",
            config.env.to_string(),
        ))
        .build()
}

/// Used when `RUST_LOG` is absent or unparseable.
const DEFAULT_FILTER: &str = "info,sqlx=warn,fred=warn,livekit_api=warn";

/// Say that `RUST_LOG` was unusable, before any subscriber can carry the message.
///
/// The subscriber is not installed yet, so a `tracing` event here goes nowhere. Written
/// through `std::io` directly and only on this path, which keeps the denial of
/// `print_stderr` honest: this is not debugging output, it is the one diagnostic a
/// misconfigured process has left.
fn filter_fallback_note(filter: &str, error: &impl std::fmt::Display) {
    use std::io::Write as _;

    let _ = writeln!(
        std::io::stderr(),
        "RUST_LOG=`{filter}` is invalid ({error}); using `{DEFAULT_FILTER}`"
    );
}

/// The filter that will be used for `config`'s environment.
///
/// Exposed so a test can assert the fallback is applied without installing a global
/// subscriber, which would conflict with the test harness.
#[must_use]
pub fn filter_for(config: &Config) -> String {
    EnvFilter::try_new(&config.log_filter)
        .map(|_| config.log_filter.clone())
        .unwrap_or_else(|_| DEFAULT_FILTER.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(filter: &str, env: &str) -> Config {
        Config::from_source(
            &crate::config::MapSource::new()
                .with("ENV", env)
                .with("DATABASE_URL", "postgres://u:p@localhost/meno")
                .with("REDIS_URL", "redis://localhost:6379")
                .with("JWT_SECRET", "a-real-secret-value")
                .with("JWT_REFRESH_SECRET", "another-real-secret")
                .with("CORS_ORIGINS", "https://app.example.com")
                .with("RUST_LOG", filter)
                .with("STORAGE_ENABLED", "false"),
        )
        .expect("valid config")
    }

    #[test]
    fn a_valid_rust_log_is_used_as_given() {
        let config = config_with("info,sqlx=warn", "dev");
        assert_eq!(filter_for(&config), "info,sqlx=warn");
    }

    #[test]
    fn a_malformed_rust_log_falls_back_instead_of_failing() {
        // A typo in RUST_LOG must not stop the process booting — the same class of bug
        // as the config layer dying on the first missing variable.
        //
        // `%%%` is genuinely invalid. Note that a *sentence* is not: `EnvFilter`
        // accepts whitespace-separated bare words as target names, so
        // "this is not a filter" parses as four targets. A test using one would pass
        // vacuously.
        for malformed in ["%%%", "=info", "a=b=c=d", "!!!"] {
            let config = config_with(malformed, "dev");
            assert_eq!(
                filter_for(&config),
                DEFAULT_FILTER,
                "`{malformed}` should fall back"
            );
        }
    }

    #[test]
    fn whitespace_separated_words_are_valid_targets_not_an_error() {
        // Documents the subtlety above so nobody "fixes" the fallback test by using a
        // sentence, which would make it pass for the wrong reason.
        let config = config_with("info sqlx=warn", "dev");
        assert_eq!(filter_for(&config), "info sqlx=warn");
    }

    #[test]
    fn the_default_filter_quiesces_the_noisy_dependencies() {
        // `fred` and `sqlx` are chatty at info level and drown out everything else.
        let filter = EnvFilter::new(DEFAULT_FILTER);
        assert!(filter.to_string().contains("sqlx=warn"));
        assert!(filter.to_string().contains("fred=warn"));
    }

    #[test]
    fn the_default_filter_itself_parses() {
        // The fallback must not itself be the thing that is broken.
        assert!(EnvFilter::try_new(DEFAULT_FILTER).is_ok());
    }

    #[test]
    fn dev_and_prod_use_different_formats() {
        // JSON for the log drain, pretty for the developer. Both must be reachable.
        let dev = config_with("info", "dev");
        let prod = config_with("info", "prod");

        assert!(!dev.env.is_deployed());
        assert!(prod.env.is_deployed());
        // The filter is identical; only the formatter differs, so this asserts the
        // branch that selects it is driven by `env`.
        assert_eq!(filter_for(&dev), filter_for(&prod));
    }

    #[test]
    fn an_unset_endpoint_means_no_export_at_all() {
        // The development default: no collector, no network calls, no guard to hold.
        let config = config_with("info", "dev");
        assert!(config.otel.is_none());
    }

    #[test]
    fn a_configured_endpoint_is_carried_into_the_settings() {
        let config = Config::from_source(
            &crate::config::MapSource::new()
                .with("ENV", "dev")
                .with("DATABASE_URL", "postgres://u:p@localhost/meno")
                .with("REDIS_URL", "redis://localhost:6379")
                .with("JWT_SECRET", "a-real-secret-value")
                .with("JWT_REFRESH_SECRET", "another-real-secret")
                .with("CORS_ORIGINS", "https://app.example.com")
                .with("OTEL_EXPORTER_OTLP_ENDPOINT", "http://localhost:4317")
                .with("OTEL_TRACES_SAMPLER_ARG", "0.25")
                .with("OTEL_SERVICE_NAME", "meno-api-dev"),
        )
        .expect("valid config");

        let otel = config.otel.expect("otel settings");
        assert_eq!(otel.endpoint, "http://localhost:4317");
        assert_eq!(otel.service_name, "meno-api-dev");
        assert!((otel.sample_ratio - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn a_trace_id_is_absent_without_an_otel_layer() {
        // `current_trace_id` is called on every request line; it must answer rather
        // than panic when OTLP is off, which is the default in development and tests.
        assert!(current_trace_id().is_none());
    }
}
