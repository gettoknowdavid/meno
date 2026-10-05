//! Shared harness for the integration tests in this directory.
//!
//! # Why this is a module and not a crate
//!
//! Cargo compiles every `tests/*.rs` as its own crate but ignores subdirectories, so
//! `tests/support/mod.rs` is not a test target — it is included by the files that are.
//! That is the standard Rust idiom for sharing fixtures without publishing them.
//!
//! # What these tests can and cannot prove
//!
//! Everything here runs against [`InMemoryAuthRepo`] and [`InMemoryAuthCache`], so it
//! exercises the *wiring*: that a real [`AuthService`] given real collaborators keeps
//! its security properties, and that the handlers turn those refusals into the right
//! status codes. It deliberately does **not** exercise `pg.rs` or `redis_store.rs` —
//! no SQL statement and no Redis command runs here. Those two adapters are covered by
//! `auth_repository_pg.rs`, which needs a live Postgres.
//!
//! So a green run of this suite means "the module is correctly assembled and correctly
//! behaved", not "the module works against production infrastructure". The distinction
//! is the reason the two suites exist separately rather than one suite pretending to
//! cover both.
//!
// The three panicking lints are denied workspace-wide but explicitly *allowed in
// tests* — see `clippy.toml`, whose comment states the policy as "deny in
// apps/api/src, allow in tests/". The `allow-*-in-tests` keys there cover `#[test]`
// functions; the shared helpers in `support/mod.rs` and the small `async fn` helpers
// below are not `#[test]` functions, so the allowance does not reach them.
//
// This is the same policy applied to the same code, not a relaxation of it: the rule
// exists so a *request path* cannot panic, and nothing here serves a request. Every
// `expect` below is a fixture that would make the test meaningless if it failed, and
// its message says so.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! # Why the doubles are reachable at all
//!
//! [`InMemoryAuthRepo`] is gated on the `test-support` feature, because from an
//! external test crate `#[cfg(test)]` is false. See `Cargo.toml`.

#![allow(dead_code)] // Each test binary uses a different subset of this harness.

use std::sync::Arc;

use meno_api::config::Secret;
use meno_api::modules::auth::cache::{AuthCache, InMemoryAuthCache};
use meno_api::modules::auth::credentials::{CredentialDeps, CredentialService};
use meno_api::modules::auth::dto::RegisterRequest;
use meno_api::modules::auth::google::{GoogleExchange, StubExchange};
use meno_api::modules::auth::mailer::{AuthEmailRequest, AuthMailer, RecordingAuthMailer};
use meno_api::modules::auth::model::{AuthProvider, User};
use meno_api::modules::auth::repository::{AuthRepo, InMemoryAuthRepo};
use meno_api::modules::auth::services::{AuthDeps, AuthService};
use meno_api::modules::auth::state::AuthState;
use meno_api::modules::auth::token::{TokenConfig, TokenService};

/// The password every fixture account is given.
///
/// Satisfies the composition policy, so a registration test reaches the auth flow
/// rather than stopping at `RegisterRequest::validate`.
pub const PASSWORD: &str = "Correct horse battery staple";

/// Everything a test needs, with the doubles left concrete so it can read them.
pub struct Harness {
    /// The wired module, as the router would receive it.
    pub state: AuthState,
    /// The sign-in/session service directly, for tests that skip HTTP.
    pub service: Arc<AuthService>,
    /// The OTP/verify-email/reset service directly.
    pub credentials: Arc<CredentialService>,
    /// Token minting, for building a second device without going through login.
    pub tokens: TokenService,
    /// The repository, typed so its session and OTP queries can be asserted on.
    pub repo: Arc<InMemoryAuthRepo>,
    /// The mailer, typed so a test can assert what was *sent*.
    pub mailer: Arc<RecordingAuthMailer>,
    /// The Google seam and its call counters.
    pub google: Arc<StubExchange>,
}

/// A token configuration that passes [`TokenConfig::validate`].
///
/// Two *different* secrets, because §4.7 item 4 is the rule that a refresh token
/// cannot be presented as an access token, and `validate` refuses a shared pair. A
/// fixture that reused one secret would be testing a configuration the code rejects.
fn token_config() -> TokenConfig {
    TokenConfig::validate(
        Secret::new("integration-access-secret"),
        Secret::new("integration-refresh-secret"),
        900,
        2_592_000,
    )
    .expect("the fixture token configuration is valid")
}

/// Build a harness whose Google provider signs `subject` in as `email`.
#[must_use]
pub fn harness() -> Harness {
    harness_with(StubExchange::exchanging(
        "google-subject",
        "ada@example.com",
    ))
}

/// Build a harness with a specific Google provider.
#[must_use]
pub fn harness_with(google: StubExchange) -> Harness {
    let repo = Arc::new(InMemoryAuthRepo::new());
    let cache = Arc::new(InMemoryAuthCache::new());

    // Concrete here, `dyn` everywhere else: the tests read `count()` and `last()` off
    // the mailer, and the services only ever see the trait. Cloning one `Arc` rather
    // than constructing a second double is what makes `mailer.count()` mean anything.
    let mailer = Arc::new(RecordingAuthMailer::new());
    let mailer_dyn: Arc<dyn AuthMailer> = Arc::clone(&mailer) as Arc<dyn AuthMailer>;

    let repo_trait: Arc<dyn AuthRepo> = Arc::clone(&repo) as Arc<dyn AuthRepo>;
    let cache_trait: Arc<dyn AuthCache> = Arc::clone(&cache) as Arc<dyn AuthCache>;

    let tokens = TokenService::new(
        token_config(),
        Arc::clone(&repo_trait),
        Arc::clone(&cache_trait),
    )
    .expect("the fixture token service is valid");

    let service = AuthService::new(AuthDeps {
        repo: Arc::clone(&repo_trait),
        tokens: tokens.clone(),
        mailer: Arc::clone(&mailer_dyn),
    })
    .expect("the fixture auth service is valid");

    let credentials = CredentialService::new(CredentialDeps {
        repo: Arc::clone(&repo_trait),
        tokens: tokens.clone(),
        mailer: Arc::clone(&mailer_dyn),
    });

    let google = Arc::new(google);
    let state = AuthState::from_parts(
        repo_trait,
        cache_trait,
        tokens.clone(),
        Arc::clone(&google) as Arc<dyn GoogleExchange>,
        mailer_dyn,
    )
    .expect("the fixture state is valid");

    Harness {
        state,
        service: Arc::new(service),
        credentials: Arc::new(credentials),
        tokens,
        repo,
        mailer,
        google,
    }
}

/// A registration request for `Ada@Example.com`.
///
/// The mixed-case address is deliberate: §9.5 normalises at the boundary, so a second
/// registration of the *same* request must collide rather than create a second account.
#[must_use]
pub fn register_request() -> RegisterRequest {
    RegisterRequest {
        full_name: "Ada Lovelace".to_owned(),
        email: "Ada@Example.com".to_owned(),
        password: PASSWORD.to_owned(),
    }
}

/// Register an account and return it with the refresh token from its first session.
///
/// # Panics
///
/// If registration fails, which would mean the fixture itself is broken.
pub async fn registered(h: &Harness) -> (User, String) {
    h.service
        .register(&register_request())
        .await
        .expect("registering the fixture account succeeds");

    let user = h
        .repo
        .find_user_by_email("ada@example.com")
        .await
        .expect("the lookup succeeds")
        .expect("the account exists");

    // `register` returns a session, so take the refresh token from it rather than
    // minting a second one: this is the pair a real client would hold.
    let refresh = h
        .tokens
        .issue_pair(&user, vec![AuthProvider::Password], Default::default())
        .await
        .expect("issuing a fixture session")
        .refresh_token;

    (user, refresh)
}

/// The last message the mailer recorded, or `None` if it sent nothing.
#[must_use]
pub fn last_mail(h: &Harness) -> Option<AuthEmailRequest> {
    h.mailer.last()
}
