//! Tracing setup.
//!
//! Ported from `apps/api/src/shared/telemetry.rs` on `master` (`903c3ba`), with the log
//! filter read from the validated [`Config`] rather than re-read from the environment.
//!
//! Two changes worth naming:
//!
//! - **The filter comes from `Config`.** `master` called `EnvFilter::try_from_default_env`
//!   here, so `RUST_LOG` was read in two places with two different fallbacks. One
//!   source, validated at startup, means the filter in the logs is the filter that was
//!   configured.
//! - **Output format follows `Config::env`, not a re-read of `ENV`.** Same reason.
//!
//! JSON in deployed environments because Render's log drain parses it; pretty in
//! development because a human is reading it.

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::Config;

/// Install the global tracing subscriber.
///
/// # Panics
///
/// Panics if a subscriber is already installed, which happens when a test binary calls
/// it twice. Nothing in this crate does — the binaries call it once at startup — and a
/// double install is a programming error worth failing loudly on rather than
/// silently ignoring.
pub fn init(config: &Config) {
    let filter = EnvFilter::try_new(&config.log_filter).unwrap_or_else(|e| {
        // A malformed `RUST_LOG` must not stop the process. Fall back to something
        // quiet and say so, rather than booting with no logs and no explanation.
        tracing::warn!(
            error = %e,
            filter = %config.log_filter,
            "invalid RUST_LOG; falling back to a default filter"
        );
        EnvFilter::new(DEFAULT_FILTER)
    });

    let registry = tracing_subscriber::registry().with(filter);

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
}

/// Used when `RUST_LOG` is absent or unparseable.
const DEFAULT_FILTER: &str = "info,sqlx=warn,fred=warn,livekit_api=warn";

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
}
