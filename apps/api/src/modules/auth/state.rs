//! What the auth module needs at runtime, already wired (plan §5.6).
//!
//! # This is the only file that names a concrete adapter
//!
//! §9.3 says state structs expose traits, not concrete types, to handlers. The way that
//! is actually enforced is this file: [`PgAuthRepo`], [`RedisAuthCache`],
//! [`ProviderExchange`] and [`TokenService`] appear *here* and nowhere above. A service
//! or handler that accidentally reached for `sqlx::PgPool` instead of
//! [`AuthRepo`](super::repository::AuthRepo) would not fail review, it would fail to
//! compile — which is the point.
//!
//! # Construction is fallible, and says why
//!
//! [`AuthState::new`] returns [`Result`] because three of its four steps genuinely can
//! fail: a misconfigured JWT pair, an unusable `GOOGLE_REDIRECT_URI`, and the Argon2id
//! dummy hash that equalises login timing. `master` expressed the first as
//! `assert!` and the second as `.expect("Invalid Google redirect URI")` — a typo in one
//! `.env` variable became a panic trace at boot (§9.1, §7.11). Here it is a startup
//! error with a readable message.
//!
//! # What is deliberately *not* built here
//!
//! The OAuth state store is a parameter rather than something this file constructs. Only
//! the process that owns Redis can build the right one, and an in-memory default would
//! compile everywhere and split the CSRF binding across replicas in production. Same for
//! the mailer, which is §4.6's optional integration: a deployment with no SMTP host
//! gets [`NoopAuthMailer`], which logs and drops, because refusing to start would mean
//! `SMTP_HOST` is load-bearing for an application that does not need email.

use std::sync::Arc;

use crate::config::Config;
use crate::infrastructure::redis::Redis;

use super::cache::{AuthCache, RedisAuthCache};
use super::credentials::{CredentialDeps, CredentialService};
use super::google::{GoogleExchange, ProviderExchange};
use super::mailer::brevo::{BrevoMailer, MailError};
use super::mailer::{AuthMailer, NoopAuthMailer};
use super::repository::{AuthRepo, PgAuthRepo, RepoDeps};
use super::services::{AuthDeps as ServiceDeps, AuthService};
use super::token::{TokenConfig, TokenService};
use meno_core::Error as MenoError;

/// Everything the auth endpoints need, wired and ready.
///
/// Cheap to clone — every field is an `Arc` or a `TokenService` of `Arc`s — so axum's
/// `State` extractor holds one per request without meaningful cost.
#[derive(Clone)]
pub struct AuthState {
    /// Registration, login, refresh, logout, sessions, Google sign-in.
    pub service: Arc<AuthService>,
    /// One-time codes, verification, password resets.
    pub credentials: Arc<CredentialService>,
    /// The Google seam, for the three endpoints that need a provider.
    pub google: Arc<dyn GoogleExchange>,
}

impl std::fmt::Debug for AuthState {
    /// `NoopAuthMailer` and friends hold nothing sensitive, but a `Debug` line for this
    /// object would say nothing useful either.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthState")
            .field("service", &self.service)
            .field("credentials", &self.credentials)
            .field("google", &self.google)
            .finish_non_exhaustive()
    }
}

/// Everything [`AuthState::new`] needs that this module cannot build for itself.
///
/// Named `Wiring` rather than `AuthDeps` because [`super::services::AuthDeps`] already
/// means "the three things [`AuthService`] needs" — two structs with the same name and
/// different fields in one module is a rename waiting to happen at a call site.
#[derive(Clone)]
pub struct Wiring {
    /// The pool, for the Postgres repository.
    pub pool: sqlx::PgPool,
    /// Redis, for the token blocklist.
    pub redis: Redis,
    /// Validated configuration (§4.6).
    pub config: Arc<Config>,
    /// Where single-use OAuth state is kept. Redis in production.
    pub oauth_states: Arc<dyn crate::infrastructure::oauth::OAuthStateStore>,
    /// Transactional mail. [`NoopAuthMailer`] when `SMTP_HOST` is unset.
    pub mailer: Arc<dyn AuthMailer>,
}

impl AuthState {
    /// Wire the module.
    ///
    /// # Errors
    ///
    /// - [`TokenConfig::validate`] — a blank, placeholder or shared JWT secret, or a
    ///   non-positive lifetime. §4.7 item 4 keeps the two secrets distinct, and this is
    ///   where that becomes an error rather than a convention.
    /// - [`ProviderExchange::from_config`] — Google sign-in is enabled but the settings
    ///   do not build a client.
    /// - [`password::dummy_hash`] — the timing-equalising
    ///   hash could not be computed, so login timing could not be equalised.
    ///
    /// All three are startup failures by design. §7.11's rule is that a misconfiguration
    /// surfaces at boot with a readable message rather than as a stack trace, and the
    /// alternative — booting anyway — turns each of them into a one-request-at-a-time
    /// outage discovered by a user.
    pub fn new(deps: Wiring) -> Result<Self, MenoError> {
        let config = deps.config.clone();

        let repo: Arc<dyn AuthRepo> = Arc::new(PgAuthRepo::new(RepoDeps { pool: deps.pool }));
        let cache: Arc<dyn AuthCache> = Arc::new(RedisAuthCache::new(deps.redis));

        let token_config = TokenConfig::validate(
            config.jwt_secret.clone(),
            config.jwt_refresh_secret.clone(),
            config.access_token_expiration.as_secs() as i64,
            config.refresh_token_expiration.as_secs() as i64,
        )?;
        let tokens = TokenService::new(token_config, repo.clone(), cache)?;

        let google: Arc<dyn GoogleExchange> =
            Arc::new(ProviderExchange::from_config(&config, deps.oauth_states)?);

        let service = Arc::new(AuthService::new(ServiceDeps {
            repo: repo.clone(),
            tokens: tokens.clone(),
            mailer: deps.mailer.clone(),
        })?);

        let credentials = Arc::new(CredentialService::new(CredentialDeps {
            repo,
            tokens,
            mailer: deps.mailer,
        }));

        Ok(Self {
            service,
            credentials,
            google,
        })
    }

    /// Wire the module from trait objects, with no infrastructure.
    ///
    /// The constructor the handler tests use, and the one any future in-process wiring
    /// (background jobs acting on a user's behalf) should prefer. Every dependency is a
    /// `dyn`, so a test can pass a repository with one user in it and a cache that
    /// remembers nothing — which is how §10's handler contract tests run without a
    /// database, a Redis or an SMTP server.
    ///
    /// # Errors
    ///
    /// [`MenoError::Internal`] if the timing-equalising dummy hash cannot be computed,
    /// for the reason on [`Self::new`].
    pub fn from_parts(
        repo: Arc<dyn AuthRepo>,
        _cache: Arc<dyn AuthCache>,
        tokens: TokenService,
        google: Arc<dyn GoogleExchange>,
        mailer: Arc<dyn AuthMailer>,
    ) -> Result<Self, MenoError> {
        Ok(Self {
            service: Arc::new(AuthService::new(ServiceDeps {
                repo: repo.clone(),
                tokens: tokens.clone(),
                mailer: mailer.clone(),
            })?),
            credentials: Arc::new(CredentialService::new(CredentialDeps {
                repo,
                tokens,
                mailer,
            })),
            google,
        })
    }

    /// The mailer for this configuration (§4.6, §3.6).
    ///
    /// A function rather than something [`Self::new`] decides internally, because the
    /// decision belongs to whoever is assembling the application: an in-process test
    /// wants [`super::mailer::RecordingAuthMailer`], and a deployment with `SMTP_HOST`
    /// set wants the real sender.
    ///
    /// - `SMTP_HOST` set → [`BrevoMailer`], the HTTPS transport plan §3.6 asks for.
    /// - `SMTP_HOST` unset → [`NoopAuthMailer`], which logs and drops: §4.6's rule is
    ///   that a *disabled* integration must not fail the flows that would use it, so
    ///   refusing to boot over a missing mail host would make `SMTP_HOST`
    ///   load-bearing for an application that does not send email.
    ///
    /// # Errors
    ///
    /// [`MailError`] when mail *is* configured but the adapter cannot be built — the
    /// shared HTTP client or the endpoint. An enabled-but-unusable adapter is exactly
    /// what §4.6 says must not ship: it would fail one send at a time forever, with
    /// the first report coming from a user who never received their code. So this is a
    /// startup failure, surfaced by `bootstrap`.
    pub fn default_mailer(config: &Config) -> Result<Arc<dyn AuthMailer>, MailError> {
        let Some(settings) = &config.email else {
            tracing::info!("no SMTP_HOST configured; auth emails are discarded");
            return Ok(Arc::new(NoopAuthMailer));
        };

        tracing::info!(
            host = %settings.host,
            "auth email configured; sending through the Brevo HTTPS adapter"
        );
        Ok(Arc::new(BrevoMailer::new(settings)?))
    }
}
