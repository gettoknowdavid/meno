//! Typed, validated application configuration.
//!
//! Replaces `master`'s `Config::from_env`, which §4.6 and §7.8 identify as a startup
//! blocker. Three defects are fixed here, and they are all the same defect wearing
//! different hats: **the config layer reported the first problem and stopped.**
//!
//! 1. **Fail-fast on the first missing variable.** A developer with an incomplete
//!    `.env` fixed `JWT_SECRET`, restarted, hit `JWT_REFRESH_SECRET`, restarted again.
//!    [`Config::load`] collects *every* problem and reports them together, so the
//!    first run tells you the whole list.
//! 2. **`FIREBASE_SERVICE_ACCOUNT_PATH` hard-failed the whole boot** though only push
//!    notifications needed it (§4.6). Every optional integration is now gated behind
//!    a flag and only read when that flag is on.
//! 3. **`origins` was hardcoded to `yourdomain.com`**, so every real frontend request
//!    was CORS-rejected. It comes from `CORS_ORIGINS` now.
//!
//! Also per §4.6: secrets are wrapped in [`Secret`], which has no `Debug` that
//! reveals the inner value, so `tracing::info!("{config:?}")` cannot leak a JWT
//! signing key into a log aggregator.
//!
//! # Required vs optional
//!
//! §4.6 fixes this list explicitly:
//!
//! **Required** — `DATABASE_URL`, `REDIS_URL`, `JWT_SECRET`, `JWT_REFRESH_SECRET`,
//! `CORS_ORIGINS`, `ENV`.
//!
//! **Gated** — `LIVEKIT_*`, `PUSH_*`/Firebase, `SMTP_*`, `STORAGE_*`. A gated
//! integration that is enabled but incompletely configured *is* an error; one that is
//! disabled is not read at all.

use std::collections::BTreeMap;
use std::fmt;
use std::time::Duration;

use serde::Deserialize;

/// The application environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Env {
    /// Local development. Verbose logs, relaxed CORS defaults.
    Dev,
    /// Shared staging.
    Staging,
    /// Production. Must not be `dev`.
    Prod,
}

impl Env {
    /// Whether this environment is anything other than local development.
    #[must_use]
    pub const fn is_deployed(self) -> bool {
        !matches!(self, Self::Dev)
    }
}

impl fmt::Display for Env {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Dev => "dev",
            Self::Staging => "staging",
            Self::Prod => "prod",
        })
    }
}

/// A string that must never be logged.
///
/// `Debug` prints `[redacted]`, so a whole-config `{:?}` is safe. `Display` is *not*
/// implemented on purpose: printing a secret requires the caller to say so explicitly
/// via [`Secret::expose`], which makes each leak a deliberate act with a grep trail.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wrap a secret value.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Read the value. Named for what it does, so call sites read as intent.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether the secret is empty or obviously a placeholder.
    ///
    /// Checked so `JWT_SECRET=` in a `.env` — which `dotenv` accepts happily and
    /// which `master` would have used to sign tokens — is a startup error rather than
    /// an authentication vulnerability.
    #[must_use]
    pub fn looks_unset(&self) -> bool {
        let trimmed = self.0.trim();
        trimmed.is_empty()
            || matches!(
                trimmed.to_ascii_lowercase().as_str(),
                "changeme" | "change_me" | "secret" | "password" | "placeholder"
            )
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

/// Everything that went wrong while loading configuration.
///
/// Aggregated rather than returned one at a time: `Config::load` cannot return a
/// half-built `Config`, so collecting is the only way to report more than the first
/// problem. Keyed by variable name and sorted, so the message is stable between runs
/// (a set would reorder and defeat `assert_eq!` in tests).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ConfigErrors {
    problems: BTreeMap<String, String>,
}

impl ConfigErrors {
    /// An empty set — the success case.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `name` is missing.
    pub fn missing(&mut self, name: &str) {
        self.problems
            .insert(name.to_owned(), "is required but not set".to_owned());
    }

    /// Record that `name` holds an unusable value.
    pub fn invalid(&mut self, name: &str, why: impl fmt::Display) {
        self.problems
            .insert(name.to_owned(), format!("is invalid: {why}"));
    }

    /// Whether anything went wrong.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.problems.is_empty()
    }

    /// How many variables are wrong. Reported first so the summary is readable even
    /// when the list is long.
    #[must_use]
    pub fn len(&self) -> usize {
        self.problems.len()
    }

    /// Every offending variable, sorted by name.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.problems.keys().map(String::as_str)
    }
}

impl fmt::Display for ConfigErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} configuration problem(s):\n{}",
            self.problems.len(),
            self.problems
                .iter()
                .map(|(k, v)| format!("  - {k} {v}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

impl std::error::Error for ConfigErrors {}

/// Credentials for the LiveKit media service. Present only when enabled.
#[derive(Clone, Debug)]
pub struct LivekitSettings {
    /// LiveKit Cloud host, e.g. `https://your-project.livekit.cloud`.
    pub host: String,
    /// Public API key. Not a secret.
    pub api_key: String,
    /// Server-side signing secret. Never sent to a client.
    pub api_secret: Secret,
}

impl LivekitSettings {
    /// How long a minted participant token stays valid, in seconds.
    ///
    /// Deliberately short. Plan §7.6 notes that a leaked token is a way into a live
    /// broadcast, so the window is minutes rather than hours and the client re-mints
    /// on expiry. Expressed as seconds so it can be asserted in a `const` block —
    /// `Duration`'s comparison operators are not `const`.
    pub const TOKEN_TTL_SECS: u64 = 15 * 60;

    /// [`Self::TOKEN_TTL_SECS`] as a [`Duration`].
    #[must_use]
    pub const fn token_ttl() -> Duration {
        Duration::from_secs(Self::TOKEN_TTL_SECS)
    }

    /// Ceiling on participants in one room. LiveKit's own default is far higher and
    /// would let one room exhaust the free tier's concurrency budget.
    pub const MAX_PARTICIPANTS: u32 = 1_000;
}

/// Firebase Cloud Messaging credentials. Present only when push is enabled.
#[derive(Clone, Debug)]
pub struct PushSettings {
    /// Firebase project id.
    pub project_id: String,
    /// The raw service-account JSON, read from `FIREBASE_SERVICE_ACCOUNT_JSON`.
    ///
    /// §7.8 notes the old code read a *path* from a variable `.env.example` named as
    /// if it held a URL. Reading the JSON directly removes the filesystem dependency,
    /// which also means it works identically on Render and in Docker.
    pub service_account_json: Secret,
}

/// Google OAuth settings. Present only when Google sign-in is enabled.
///
/// Optional per §4.6: a deployment that authenticates by email and password alone must
/// not be blocked by five unset `GOOGLE_*` variables. Gated on `GOOGLE_ENABLED`, or —
/// like storage — on the presence of `GOOGLE_CLIENT_ID` when the flag is absent, because
/// `.env.example` declares the variables without a flag.
#[derive(Clone, Debug)]
pub struct GoogleSettings {
    /// OAuth client id.
    pub client_id: String,

    /// OAuth client secret.
    ///
    /// A [`Secret`] because `GoogleSettings` is `Debug` and this lands in adapter state
    /// that may be logged (§9.5). A derived `Debug` would print it.
    pub client_secret: Secret,

    /// Where the provider sends the user back after consent.
    pub redirect_uri: String,

    /// The consent endpoint. Overridable so tests can point at a local server.
    pub auth_uri: String,

    /// The token endpoint. Overridable for the same reason.
    pub token_uri: String,

    /// The OpenID Connect userinfo endpoint.
    pub userinfo_uri: String,

    /// Google's post-hoc ID-token inspection endpoint.
    pub tokeninfo_uri: String,
}

impl GoogleSettings {
    /// Google's production consent endpoint.
    pub const DEFAULT_AUTH_URI: &'static str = "https://accounts.google.com/o/oauth2/v2/auth";

    /// Google's production token endpoint.
    pub const DEFAULT_TOKEN_URI: &'static str = "https://oauth2.googleapis.com/token";

    /// Google's production userinfo endpoint.
    pub const DEFAULT_USERINFO_URI: &'static str =
        "https://openidconnect.googleapis.com/v1/userinfo";

    /// Google's production token-introspection endpoint.
    pub const DEFAULT_TOKENINFO_URI: &'static str = "https://oauth2.googleapis.com/tokeninfo";
}

/// SMTP/transactional-email settings. Present only when email is enabled.
#[derive(Clone, Debug)]
pub struct EmailSettings {
    /// SMTP host.
    pub host: String,
    /// SMTP port.
    pub port: u16,
    /// Username.
    pub user: String,
    /// Password.
    pub password: Secret,
    /// Envelope sender.
    pub from: String,
}

/// S3-compatible object storage settings. Present only when storage is enabled.
#[derive(Clone, Debug)]
pub struct StorageSettings {
    /// Endpoint URL. `http://` on the RustFS container, `https://` on R2.
    pub endpoint: String,
    /// Access key id.
    pub access_key: String,
    /// Secret key.
    pub secret_key: Secret,
    /// Bucket name.
    pub bucket: String,
    /// Region.
    pub region: String,
    /// Public base URL objects are served from.
    pub public_url: String,
}

/// The validated application configuration.
///
/// Built once, at startup, and passed by reference thereafter. Every field is either
/// required (§4.6) or sits behind an `Option` whose feature flag has already been
/// checked.
#[derive(Clone, Debug)]
pub struct Config {
    // ── required (§4.6) ──
    /// Which environment this is.
    pub env: Env,
    /// TCP port for the HTTP server.
    pub port: u16,
    /// Postgres connection string.
    pub database_url: Secret,
    /// Redis connection string.
    pub redis_url: Secret,
    /// Access-token signing secret.
    pub jwt_secret: Secret,
    /// Refresh-token signing secret.
    pub jwt_refresh_secret: Secret,
    /// Allowed CORS origins, parsed from `CORS_ORIGINS`.
    pub origins: Vec<String>,

    // ── token lifetimes ──
    /// Access-token lifetime, from `ACCESS_TOKEN_EXPIRATION`.
    pub access_token_expiration: Duration,
    /// Refresh-token lifetime, from `REFRESH_TOKEN_EXPIRATION`.
    pub refresh_token_expiration: Duration,

    // ── feature-gated (§4.6) ──
    /// LiveKit settings, when `LIVEKIT_ENABLED=true`.
    pub livekit: Option<LivekitSettings>,
    /// Push settings, when `PUSH_ENABLED=true`.
    pub push: Option<PushSettings>,
    /// Email settings, when `SMTP_HOST` is set.
    pub email: Option<EmailSettings>,
    /// Storage settings, when `STORAGE_ENABLED=true`.
    pub storage: Option<StorageSettings>,
    /// Google OAuth settings, when `GOOGLE_ENABLED=true`.
    pub google: Option<GoogleSettings>,

    // ── operational ──
    /// `tracing` filter directive, from `RUST_LOG`.
    pub log_filter: String,
    /// Whether the web binary may skip migrations, from `SKIP_MIGRATIONS`.
    pub skip_migrations: bool,
    /// Static bearer token guarding `GET /metrics`, from `METRICS_TOKEN`.
    ///
    /// Optional and fail-closed (§7.5, §4.8): **unset means the endpoint answers
    /// 404**. The plan is explicit that `/metrics` must be private, and where a
    /// private bind is unavailable a static token is the middle option it names —
    /// but a token nobody set must disable the endpoint, not leave it open. `None`
    /// is therefore the safe default, and `.env.example` says so next to the key.
    pub metrics_token: Option<Secret>,
}

/// Where configuration values are read from.
///
/// Split out from [`Config::load`] so the parsing and validation logic is testable
/// without mutating the process's environment — `std::env::set_var` is global, racy
/// under `cargo test`'s parallel threads, and `unsafe` in edition 2024. Production
/// reads the real environment; tests read a `BTreeMap`.
pub trait ConfigSource {
    /// Look up a variable. `None` means unset.
    fn get(&self, key: &str) -> Option<String>;
}

/// Reads from the process environment.
#[derive(Clone, Copy, Debug, Default)]
pub struct EnvSource;

impl ConfigSource for EnvSource {
    fn get(&self, key: &str) -> Option<String> {
        // An empty value counts as unset. `dotenv` and most CI secret stores write
        // `KEY=` for an absent secret, and treating that as a real value produces the
        // "valid but empty" configuration that `Secret::looks_unset` then rejects
        // with a much less obvious message.
        std::env::var(key).ok().filter(|v| !v.trim().is_empty())
    }
}

/// Reads from an in-memory map. For tests.
#[derive(Clone, Debug, Default)]
pub struct MapSource(BTreeMap<String, String>);

impl MapSource {
    /// An empty source.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a variable.
    #[must_use]
    pub fn with(mut self, key: &str, value: &str) -> Self {
        self.0.insert(key.to_owned(), value.to_owned());
        self
    }
}

impl ConfigSource for MapSource {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned().filter(|v| !v.trim().is_empty())
    }
}

impl Config {
    /// Load from the process environment, first letting `dotenvy` populate it.
    ///
    /// # Errors
    ///
    /// Returns every missing or invalid variable at once — see [`ConfigErrors`].
    pub fn load() -> Result<Self, ConfigErrors> {
        dotenvy::dotenv().ok();
        Self::from_source(&EnvSource)
    }

    /// Validate a configuration from an arbitrary source.
    ///
    /// The whole of the real logic; [`Self::load`] is this plus the environment.
    ///
    /// # Errors
    ///
    /// Aggregates every problem rather than stopping at the first, and checks the
    /// gated integrations only when their flag is on.
    pub fn from_source(source: &impl ConfigSource) -> Result<Self, ConfigErrors> {
        let mut errors = ConfigErrors::new();

        // ── required ──
        let env = match source.get("ENV").as_deref() {
            Some(raw) => match raw.to_ascii_lowercase().as_str() {
                "dev" | "development" | "local" => Env::Dev,
                "staging" | "stage" => Env::Staging,
                "prod" | "production" => Env::Prod,
                other => {
                    errors.invalid("ENV", format!("`{other}` is not dev, staging or prod"));
                    Env::Dev
                }
            },
            None => {
                errors.missing("ENV");
                Env::Dev
            }
        };

        let port = match source.get("PORT") {
            Some(raw) => match raw.parse::<u16>() {
                Ok(0) => {
                    errors.invalid("PORT", "0 is not a bindable port");
                    8080
                }
                Ok(parsed) => parsed,
                Err(e) => {
                    errors.invalid("PORT", e);
                    8080
                }
            },
            None => 8080,
        };

        let database_url = required_secret(source, "DATABASE_URL", &mut errors);
        let redis_url = required_secret(source, "REDIS_URL", &mut errors);
        let jwt_secret = required_secret(source, "JWT_SECRET", &mut errors);
        let jwt_refresh_secret = required_secret(source, "JWT_REFRESH_SECRET", &mut errors);

        let origins = parse_origins(source, &mut errors);

        // ── token lifetimes ──
        let access_token_expiration =
            parse_duration_secs(source, "ACCESS_TOKEN_EXPIRATION", 900, &mut errors);
        let refresh_token_expiration =
            parse_duration_secs(source, "REFRESH_TOKEN_EXPIRATION", 604_800, &mut errors);

        // ── gated integrations ──
        let livekit_enabled = parse_bool(source, "LIVEKIT_ENABLED", false);
        let livekit = if livekit_enabled {
            // `LIVEKIT_URL` is the current name; `LIVEKIT_HOST` is what the pre-0.8
            // SDK and the existing `.env.example` use. Accepting both means a
            // deployment that has been working keeps working across the SDK upgrade,
            // and `.env.example` listed both.
            let host = match (source.get("LIVEKIT_URL"), source.get("LIVEKIT_HOST")) {
                (Some(url), _) => url,
                (None, Some(legacy)) => legacy,
                (None, None) => {
                    errors.missing("LIVEKIT_URL");
                    String::new()
                }
            };
            Some(LivekitSettings {
                host,
                api_key: required_str(source, "LIVEKIT_API_KEY", &mut errors),
                api_secret: required_secret(source, "LIVEKIT_API_SECRET", &mut errors),
            })
        } else {
            None
        };

        let push_enabled = parse_bool(source, "PUSH_ENABLED", false);
        let push = if push_enabled {
            Some(PushSettings {
                project_id: required_str(source, "FIREBASE_PROJECT_ID", &mut errors),
                // The JSON arrives inline on Render and in compose; reading it as a
                // filesystem path (as `master` did) meant a variable that existed but
                // could never be read.
                service_account_json: required_secret(
                    source,
                    "FIREBASE_SERVICE_ACCOUNT_JSON",
                    &mut errors,
                ),
            })
        } else {
            None
        };

        // Email is gated on presence rather than a flag: there is no `EMAIL_ENABLED`
        // in `.env.example`, and inferring from `SMTP_HOST` keeps local boot working
        // when mail is simply not configured.
        let email = source.get("SMTP_HOST").map(|host| EmailSettings {
            host,
            port: match source.get("SMTP_PORT") {
                Some(raw) => match raw.parse::<u16>() {
                    Ok(0) | Err(_) => {
                        errors.invalid("SMTP_PORT", "must be a valid non-zero port");
                        465
                    }
                    Ok(parsed) => parsed,
                },
                None => 465,
            },
            user: required_str(source, "SMTP_USER", &mut errors),
            password: required_secret(source, "SMTP_PASSWORD", &mut errors),
            from: required_str(source, "SMTP_FROM", &mut errors),
        });

        // Storage is gated on `STORAGE_ENABLED`, falling back to the presence of
        // `STORAGE_ENDPOINT` only when the flag is unset.
        //
        // §4.6 asks for a `STORAGE_ENABLED` flag, but `.env.example` predates it and lists
        // the six `STORAGE_*` variables with no flag, so keying solely on the flag would
        // leave a correctly-populated existing deployment reporting storage as disabled.
        // The flag still wins whenever it is present — including an explicit `false`, which
        // has to override the variables rather than being ignored by them.
        let storage_enabled = match source.get("STORAGE_ENABLED") {
            Some(raw) => matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ),
            None => source.get("STORAGE_ENDPOINT").is_some(),
        };
        let storage = if storage_enabled {
            Some(StorageSettings {
                endpoint: required_str(source, "STORAGE_ENDPOINT", &mut errors),
                access_key: required_str(source, "STORAGE_ACCESS_KEY", &mut errors),
                secret_key: required_secret(source, "STORAGE_SECRET_KEY", &mut errors),
                bucket: required_str(source, "STORAGE_BUCKET", &mut errors),
                region: required_str(source, "STORAGE_REGION", &mut errors),
                public_url: required_str(source, "STORAGE_PUBLIC_URL", &mut errors),
            })
        } else {
            None
        };

        // Google OAuth is an optional integration (§4.6) for the same reason storage is:
        // `.env.example` lists five `GOOGLE_*` variables and no `GOOGLE_ENABLED`, so a
        // populated deployment must keep working without setting one. The flag stays
        // authoritative in both directions — an explicit `false` must beat the variables.
        let google_enabled = match source.get("GOOGLE_ENABLED") {
            Some(raw) => matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            ),
            None => source.get("GOOGLE_CLIENT_ID").is_some(),
        };
        let google = if google_enabled {
            Some(GoogleSettings {
                client_id: required_str(source, "GOOGLE_CLIENT_ID", &mut errors),
                client_secret: required_secret(source, "GOOGLE_CLIENT_SECRET", &mut errors),
                redirect_uri: required_str(source, "GOOGLE_REDIRECT_URI", &mut errors),
                // The endpoints default rather than being required. They are constants in
                // production, and only a test needs to redirect them — making them
                // required would put five more variables in the startup failure path for
                // no operational benefit.
                auth_uri: optional_str(source, "GOOGLE_AUTH_URI")
                    .unwrap_or_else(|| GoogleSettings::DEFAULT_AUTH_URI.to_owned()),
                token_uri: optional_str(source, "GOOGLE_TOKEN_URI")
                    .unwrap_or_else(|| GoogleSettings::DEFAULT_TOKEN_URI.to_owned()),
                userinfo_uri: optional_str(source, "GOOGLE_USERINFO_URI")
                    .unwrap_or_else(|| GoogleSettings::DEFAULT_USERINFO_URI.to_owned()),
                tokeninfo_uri: optional_str(source, "GOOGLE_TOKENINFO_URI")
                    .unwrap_or_else(|| GoogleSettings::DEFAULT_TOKENINFO_URI.to_owned()),
            })
        } else {
            None
        };

        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(Self {
            env,
            port,
            database_url,
            redis_url,
            jwt_secret,
            jwt_refresh_secret,
            origins,
            access_token_expiration,
            refresh_token_expiration,
            livekit,
            push,
            email,
            storage,
            google,
            log_filter: source
                .get("RUST_LOG")
                .unwrap_or_else(|| "info,sqlx=warn".to_owned()),
            skip_migrations: parse_bool(source, "SKIP_MIGRATIONS", false),
            // Optional by design: an absent METRICS_TOKEN disables `/metrics` rather
            // than failing startup — see the field's docs.
            metrics_token: optional_str(source, "METRICS_TOKEN").map(Secret::new),
        })
    }

    /// Whether LiveKit is configured and usable.
    #[must_use]
    pub fn livekit_enabled(&self) -> bool {
        self.livekit.is_some()
    }

    /// A startup banner that is safe to log.
    ///
    /// Deliberately a hand-written summary rather than `{:?}` on `self`: the shape of
    /// this method is the guarantee that adding a field to `Config` cannot
    /// accidentally start printing a secret.
    #[must_use]
    pub fn summary(&self) -> String {
        let integrations = [
            ("livekit", self.livekit.is_some()),
            ("push", self.push.is_some()),
            ("email", self.email.is_some()),
            ("storage", self.storage.is_some()),
            ("google", self.google.is_some()),
        ]
        .into_iter()
        .map(|(name, on)| format!("{name}={}", if on { "on" } else { "off" }))
        .collect::<Vec<_>>()
        .join(" ");

        format!(
            "env={} port={} origins={} [{integrations}]",
            self.env,
            self.port,
            self.origins.len()
        )
    }
}

/// A variable with a fallback: absent *or* blank falls back to `None`.
///
/// Blank counts as absent because `.env` files carry `GOOGLE_AUTH_URI=` as an empty
/// assignment very often, and treating that as "set to the empty string" produces a
/// request to a relative URL that fails on every login.
fn optional_str(source: &impl ConfigSource, key: &str) -> Option<String> {
    source
        .get(key)
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn required_str(source: &impl ConfigSource, key: &str, errors: &mut ConfigErrors) -> String {
    match source.get(key) {
        Some(value) => value,
        None => {
            errors.missing(key);
            String::new()
        }
    }
}

fn required_secret(source: &impl ConfigSource, key: &str, errors: &mut ConfigErrors) -> Secret {
    match source.get(key) {
        Some(value) => {
            let secret = Secret::new(value);
            if secret.looks_unset() {
                errors.invalid(key, "must not be a placeholder value");
            }
            secret
        }
        None => {
            errors.missing(key);
            Secret::new(String::new())
        }
    }
}

fn parse_bool(source: &impl ConfigSource, key: &str, default: bool) -> bool {
    match source.get(key) {
        Some(raw) => matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        None => default,
    }
}

fn parse_duration_secs(
    source: &impl ConfigSource,
    key: &str,
    default: u64,
    errors: &mut ConfigErrors,
) -> Duration {
    match source.get(key) {
        Some(raw) => match raw.trim().parse::<u64>() {
            Ok(0) => {
                errors.invalid(key, "must be greater than zero seconds");
                Duration::from_secs(default)
            }
            Ok(seconds) => Duration::from_secs(seconds),
            Err(e) => {
                errors.invalid(key, e);
                Duration::from_secs(default)
            }
        },
        None => Duration::from_secs(default),
    }
}

/// `CORS_ORIGINS` is comma-separated. An unparseable entry is an error rather than a
/// skipped value, because a silently-dropped origin is a confusing CORS failure much
/// later.
fn parse_origins(source: &impl ConfigSource, errors: &mut ConfigErrors) -> Vec<String> {
    let Some(raw) = source.get("CORS_ORIGINS") else {
        errors.missing("CORS_ORIGINS");
        return Vec::new();
    };

    let origins: Vec<String> = raw
        .split(',')
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .map(str::to_owned)
        .collect();

    if origins.is_empty() {
        errors.invalid("CORS_ORIGINS", "must list at least one origin");
    }

    for origin in &origins {
        if !(origin.starts_with("http://") || origin.starts_with("https://")) {
            errors.invalid(
                "CORS_ORIGINS",
                format!("`{origin}` is not an http(s) origin"),
            );
        }
    }

    origins
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A source with every required variable satisfied, so each test can break
    /// exactly one thing.
    fn valid() -> MapSource {
        MapSource::new()
            .with("ENV", "dev")
            .with("PORT", "8080")
            .with("DATABASE_URL", "postgres://u:p@localhost:5432/meno")
            .with("REDIS_URL", "redis://localhost:6379")
            .with("JWT_SECRET", "a-real-secret-value")
            .with("JWT_REFRESH_SECRET", "another-real-secret-value")
            .with("CORS_ORIGINS", "https://app.example.com")
    }

    /// [`valid`] plus the six `STORAGE_*` variables `.env.example` declares.
    fn with_storage() -> MapSource {
        valid()
            .with("STORAGE_ENDPOINT", "http://localhost:9000")
            .with("STORAGE_ACCESS_KEY", "rustfsadmin")
            .with("STORAGE_SECRET_KEY", "rustfspassword")
            .with("STORAGE_BUCKET", "meno-uploads")
            .with("STORAGE_REGION", "us-east-1")
            .with("STORAGE_PUBLIC_URL", "http://localhost:9000/meno-uploads")
    }

    #[test]
    fn a_complete_environment_loads() {
        let config = Config::from_source(&with_storage()).expect("valid config");

        assert_eq!(config.env, Env::Dev);
        assert_eq!(config.port, 8080);
        assert_eq!(config.origins, vec!["https://app.example.com"]);
        assert_eq!(config.access_token_expiration, Duration::from_secs(900));
        assert_eq!(
            config.refresh_token_expiration,
            Duration::from_secs(604_800)
        );
    }

    #[test]
    fn storage_is_enabled_by_the_variables_alone() {
        // `.env.example` has six `STORAGE_*` variables and no `STORAGE_ENABLED`, so a
        // populated deployment must keep working without setting one.
        let config = Config::from_source(&with_storage()).expect("valid");
        let storage = config.storage.as_ref().expect("storage must be detected");

        assert_eq!(storage.bucket, "meno-uploads");
        assert_eq!(storage.region, "us-east-1");
    }

    #[test]
    fn an_explicit_storage_flag_wins_when_storage_is_absent() {
        // The §4.6 flag stays authoritative for anyone who sets it: enabling storage
        // without configuring it is an error, not a silent no-op.
        let source = valid().with("STORAGE_ENABLED", "true");
        let err = Config::from_source(&source).expect_err("enabled but unset must fail");
        assert!(err.to_string().contains("STORAGE_ENDPOINT"), "{err}");
    }

    #[test]
    fn storage_can_be_turned_off_explicitly() {
        let config = Config::from_source(&with_storage().with("STORAGE_ENABLED", "false"))
            .expect("storage disabled is valid");
        assert!(
            config.storage.is_none(),
            "an explicit false must win over the present variables"
        );
    }

    #[test]
    fn every_problem_is_reported_at_once_not_just_the_first() {
        // The whole point of aggregating. `master` stopped at the first missing
        // variable, so this took five restarts to discover.
        let source = MapSource::new();
        let errors = Config::from_source(&source).expect_err("must fail");

        for expected in [
            "DATABASE_URL",
            "REDIS_URL",
            "JWT_SECRET",
            "JWT_REFRESH_SECRET",
            "CORS_ORIGINS",
        ] {
            assert!(
                errors.keys().any(|k| k == expected),
                "{expected} should be reported; got {:?}",
                errors.keys().collect::<Vec<_>>()
            );
        }
        // Six: the five required secrets plus `ENV`. Nothing optional is reported,
        // because an unset optional integration is not a problem (§4.6).
        assert_eq!(errors.len(), 6, "5 required + ENV");
    }

    #[test]
    fn missing_and_invalid_are_distinguished_in_the_message() {
        let source = valid().with("JWT_SECRET", "changeme");
        let err = Config::from_source(&source).expect_err("placeholder must fail");
        let text = err.to_string();

        assert!(text.contains("JWT_SECRET is invalid"), "{text}");
        assert!(
            text.contains("placeholder"),
            "the message should say what is wrong, not just that it is wrong: {text}"
        );
    }

    #[test]
    fn an_empty_value_counts_as_unset() {
        // `KEY=` in a `.env` is the common shape of a missing secret; treating it as
        // a real value yields a config that boots and then fails authentication.
        let source = valid().with("JWT_SECRET", "");
        let err = Config::from_source(&source).expect_err("empty must fail");
        assert!(err.to_string().contains("JWT_SECRET"), "{err}");
    }

    #[test]
    fn placeholder_secrets_are_rejected() {
        // `master` would sign real tokens with any of these.
        for placeholder in ["changeme", "CHANGE_ME", "secret", "password", "  "] {
            let source = valid().with("JWT_SECRET", placeholder);
            assert!(
                Config::from_source(&source).is_err(),
                "`{placeholder}` must not be accepted as a JWT secret"
            );
        }
    }

    #[test]
    fn a_secret_never_prints_its_value() {
        // The leak this prevents: `tracing::info!("{config:?}")` writing a signing key
        // to a log aggregator that anyone on the team can search.
        let secret = Secret::new("super-secret-signing-key");
        assert_eq!(format!("{secret:?}"), "[redacted]");
        assert!(
            !format!("{secret:?}").contains("super-secret"),
            "the value must not appear in Debug output"
        );
        assert_eq!(secret.expose(), "super-secret-signing-key");
    }

    #[test]
    fn the_startup_summary_contains_no_secret_material() {
        let config = Config::from_source(
            &valid()
                .with("DATABASE_URL", "postgres://user:hunter2@localhost/meno")
                .with("JWT_SECRET", "jwt-signing-key")
                .with("JWT_REFRESH_SECRET", "refresh-signing-key"),
        )
        .expect("valid");

        let summary = config.summary();
        for leak in ["hunter2", "jwt-signing-key", "refresh-signing-key"] {
            assert!(!summary.contains(leak), "{leak} leaked into {summary}");
        }
        assert!(summary.contains("env=dev"));
    }

    #[test]
    fn optional_integrations_do_not_block_boot_when_disabled() {
        // §4.6: this is the defect that stopped local development. No LiveKit, no
        // Firebase, no SMTP — and the app must still start.
        let config = Config::from_source(&valid()).expect("must boot without integrations");

        assert!(!config.livekit_enabled());
        assert!(config.livekit.is_none());
        assert!(config.push.is_none());
        assert!(config.email.is_none());
    }

    #[test]
    fn the_legacy_livekit_host_variable_is_still_accepted() {
        // The pre-0.8 SDK and the current `.env.example` both say LIVEKIT_HOST. A
        // deployment that has been working must keep working across the upgrade.
        let config = Config::from_source(
            &valid()
                .with("LIVEKIT_ENABLED", "true")
                .with("LIVEKIT_HOST", "https://legacy.livekit.cloud")
                .with("LIVEKIT_API_KEY", "key")
                .with("LIVEKIT_API_SECRET", "a-secret"),
        )
        .expect("valid");

        assert_eq!(
            config.livekit.as_ref().expect("enabled").host,
            "https://legacy.livekit.cloud"
        );
    }

    #[test]
    fn livekit_url_wins_over_the_legacy_host_name() {
        let config = Config::from_source(
            &valid()
                .with("LIVEKIT_ENABLED", "true")
                .with("LIVEKIT_URL", "https://new.livekit.cloud")
                .with("LIVEKIT_HOST", "https://legacy.livekit.cloud")
                .with("LIVEKIT_API_KEY", "key")
                .with("LIVEKIT_API_SECRET", "a-secret"),
        )
        .expect("valid");

        assert_eq!(
            config.livekit.as_ref().expect("enabled").host,
            "https://new.livekit.cloud"
        );
    }

    #[test]
    fn an_enabled_integration_with_missing_values_does_fail() {
        // The other half of §4.6: "optional" must not become "silently broken".
        let source = valid().with("LIVEKIT_ENABLED", "true");
        let err = Config::from_source(&source).expect_err("enabled but unset must fail");

        let text = err.to_string();
        for expected in ["LIVEKIT_URL", "LIVEKIT_API_KEY", "LIVEKIT_API_SECRET"] {
            assert!(text.contains(expected), "{expected} missing from: {text}");
        }
    }

    #[test]
    fn livekit_flag_parses_the_usual_truthy_spellings() {
        for truthy in ["true", "TRUE", "1", "yes", "on"] {
            let source = valid()
                .with("LIVEKIT_ENABLED", truthy)
                .with("LIVEKIT_URL", "https://p.livekit.cloud")
                .with("LIVEKIT_API_KEY", "key")
                .with("LIVEKIT_API_SECRET", "a-secret");
            let config = Config::from_source(&source).expect("valid livekit config");
            assert!(config.livekit_enabled(), "`{truthy}` should enable LiveKit");
        }

        for falsy in ["false", "0", "no", "off"] {
            let source = valid().with("LIVEKIT_ENABLED", falsy);
            let config = Config::from_source(&source).expect("valid config");
            assert!(
                !config.livekit_enabled(),
                "`{falsy}` should disable LiveKit"
            );
        }
    }

    #[test]
    fn push_reads_the_service_account_json_inline() {
        // §7.8: `master` read `FIREBASE_SERVICE_ACCOUNT_PATH` as a filesystem path,
        // so a variable that existed could still not be used. It must work from the
        // value alone, with no file on disk.
        let source = valid()
            .with("PUSH_ENABLED", "true")
            .with("FIREBASE_PROJECT_ID", "meno-prod")
            .with(
                "FIREBASE_SERVICE_ACCOUNT_JSON",
                r#"{"type":"service_account","project_id":"meno-prod"}"#,
            );

        let config = Config::from_source(&source).expect("valid push config");
        let push = config.push.as_ref().expect("push enabled");

        assert_eq!(push.project_id, "meno-prod");
        assert!(
            push.service_account_json
                .expose()
                .contains("service_account")
        );
    }

    #[test]
    fn email_is_inferred_from_the_presence_of_an_smtp_host() {
        // `.env.example` has no `EMAIL_ENABLED`, so gating on a flag that nobody sets
        // would mean mail is silently never configured.
        let source = valid()
            .with("SMTP_HOST", "smtp.example.com")
            .with("SMTP_USER", "postmaster")
            .with("SMTP_PASSWORD", "smtp-password")
            .with("SMTP_FROM", "hello@example.com");

        let config = Config::from_source(&source).expect("valid");
        let email = config.email.as_ref().expect("email inferred");

        assert_eq!(email.host, "smtp.example.com");
        assert_eq!(email.port, 465, "465 is the documented default");
    }

    #[test]
    fn cors_origins_are_split_and_trimmed() {
        let source = valid().with(
            "CORS_ORIGINS",
            " https://app.example.com , https://staging.example.com ",
        );
        let config = Config::from_source(&source).expect("valid");

        assert_eq!(
            config.origins,
            vec!["https://app.example.com", "https://staging.example.com"]
        );
    }

    #[test]
    fn a_non_http_origin_is_rejected() {
        // A silently-accepted bad origin becomes a CORS failure with no clue as to
        // why, much later and in the browser.
        let source = valid().with("CORS_ORIGINS", "app.example.com");
        let err = Config::from_source(&source).expect_err("must reject");

        assert!(err.to_string().contains("not an http(s) origin"), "{err}");
    }

    #[test]
    fn an_unparseable_port_falls_back_and_reports() {
        let source = valid().with("PORT", "not-a-number");
        let err = Config::from_source(&source).expect_err("must reject");
        assert!(err.to_string().contains("PORT"), "{err}");
    }

    #[test]
    fn port_zero_is_rejected() {
        // Binds fine and then serves nothing reachable — a confusing production
        // symptom, so it is a startup error instead.
        let source = valid().with("PORT", "0");
        let err = Config::from_source(&source).expect_err("must reject");
        assert!(err.to_string().contains("PORT"), "{err}");
    }

    #[test]
    fn a_zero_token_expiration_is_rejected() {
        // A zero-lifetime access token would authenticate nobody, or everybody,
        // depending on how the comparison was written.
        let source = valid().with("ACCESS_TOKEN_EXPIRATION", "0");
        let err = Config::from_source(&source).expect_err("must reject");
        assert!(err.to_string().contains("ACCESS_TOKEN_EXPIRATION"), "{err}");
    }

    #[test]
    fn env_spellings_map_onto_the_three_tiers() {
        for (raw, expected) in [
            ("dev", Env::Dev),
            ("development", Env::Dev),
            ("local", Env::Dev),
            ("staging", Env::Staging),
            ("prod", Env::Prod),
            ("production", Env::Prod),
        ] {
            let config = Config::from_source(&valid().with("ENV", raw)).expect("valid");
            assert_eq!(config.env, expected, "`{raw}` should map to {expected:?}");
        }
    }

    #[test]
    fn an_unknown_env_value_is_rejected_rather_than_defaulted() {
        // Defaulting to `dev` on a typo in production is how staging ends up serving
        // relaxed CORS and verbose logs.
        let source = valid().with("ENV", "productionn");
        let err = Config::from_source(&source).expect_err("must reject");
        assert!(err.to_string().contains("ENV"), "{err}");
    }

    #[test]
    fn skip_migrations_is_off_unless_explicitly_enabled() {
        // §4.3: the escape hatch must be opt-in, or it silently becomes the default
        // and schema drift goes unnoticed.
        let config = Config::from_source(&valid()).expect("valid");
        assert!(!config.skip_migrations);

        let config = Config::from_source(&valid().with("SKIP_MIGRATIONS", "true")).expect("valid");
        assert!(config.skip_migrations);
    }

    #[test]
    fn the_error_message_lists_problems_in_a_stable_order() {
        // Sorted, so the message is diffable between runs and assertable in a test.
        let errors = Config::from_source(&MapSource::new()).expect_err("must fail");

        let keys: Vec<&str> = errors.keys().collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn a_livekit_token_lifetime_is_short_enough_to_limit_exposure() {
        // Plan §7.6: a leaked token is a way into a live broadcast. These are
        // compile-time constants, so the bound is checked at compile time.
        const {
            assert!(
                LivekitSettings::TOKEN_TTL_SECS <= 30 * 60,
                "a token valid for longer than half an hour is too long"
            );
            assert!(
                LivekitSettings::MAX_PARTICIPANTS < 10_000,
                "the free tier's concurrency budget cannot absorb LiveKit's default"
            );
        }
        assert_eq!(
            LivekitSettings::token_ttl(),
            Duration::from_secs(LivekitSettings::TOKEN_TTL_SECS),
            "token_ttl() must agree with TOKEN_TTL_SECS"
        );
    }

    #[test]
    fn deployed_environments_are_distinguishable_from_dev() {
        assert!(!Env::Dev.is_deployed());
        assert!(Env::Staging.is_deployed());
        assert!(Env::Prod.is_deployed());
    }
}
