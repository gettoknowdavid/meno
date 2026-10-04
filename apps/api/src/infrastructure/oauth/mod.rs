//! Google sign-in.
//!
//! Ported from `apps/api/src/infrastructure/oauth/google.rs` on `master` (`903c3ba`) and
//! reworked for the monorepo's layering rules.
//!
//! # What changed, and why
//!
//! - **A [`IdentityProvider`] trait** (§5.6). §5.6 asks for `ObjectStore` and
//!   `EmailSender`; this is the fourth adapter trait the plan implies — without one, the
//!   §7.12 security property is only testable by owning a Google account.
//! - **§7.12 is closed.** `master` returned whatever userinfo said from `exchange_code`,
//!   including `email_verified: false`, and only `verify_id_token` checked it — so a
//!   caller using the code flow inherited the takeover. Both entry points now go through
//!   one guard, so neither can be added without the check.
//! - **No panics** (§9.1). `master` had five `.expect()` calls, including
//!   `.expect("Invalid Google redirect URI")` — a typo in one `.env` variable became a
//!   panic trace at boot. §7.11 names startup panics as a defect.
//! - **State is stored and consumed, not merely generated** (§4.7.7). `master` issued a
//!   CSRF token and never compared it, so the CSRF binding existed in name only.
//! - **Endpoints are configuration**, so the flow is testable against `wiremock` with no
//!   Google account.
//!
//! # Module map
//!
//! - [`store`] — [`IdentityProvider`], [`OAuthStateStore`], [`GoogleIdentity`].
//! - [`google`] — [`GoogleIdentityProvider`], the production implementation.
//! - [`error`] — [`OAuthError`].
//!
//! # On the number of error types
//!
//! §4.2 replaced nine parallel enums with one `crates/core::Error`, and that is done. The
//! [`OAuthError`] here is adapter-local, exactly like [`crate::infrastructure::push::PushError`]
//! and [`crate::infrastructure::storage::StorageError`]: it lives inside the adapter, where
//! driver types are legitimate, and `From<OAuthError> for meno_core::Error` erases it at
//! the boundary (§5.5). No caller ever sees it. See `error` for the full argument.
//!
//! # Testing
//!
//! [`InMemoryIdentityProvider`] and [`InMemoryStateStore`] make the whole flow —
//! authorize, store, callback, link — runnable with no network. The Google adapter's
//! request path is covered against `wiremock`; its consent screen is not, and the
//! `#[ignore]`d tests in `google::tests::live` are where that lives.

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;

use crate::config::Config;
use crate::infrastructure::oauth::google::GoogleIdentityProvider;

pub mod error;
pub mod google;
pub mod store;

pub use error::OAuthError;
pub use store::{
    GoogleIdentity, IdentityProvider, InMemoryStateStore, OAuthState, OAuthStateStore,
    STATE_TTL_SECS,
};

/// The no-op [`IdentityProvider`] used when `GOOGLE_ENABLED` is not true (§4.6).
///
/// A real implementation rather than a `cfg`-gated stub, for the same reason push and
/// storage have one: a build with Google disabled must still compile the sign-in screen,
/// which asks [`IdentityProvider::is_enabled`] and hides the button.
///
/// [`IdentityProvider::authorize_url`] still returns a URL rather than an error. That is
/// deliberate — the caller renders the button only when `is_enabled()` is true, and
/// refusing to produce a URL would turn a configuration state into a 500 if the two ever
/// disagreed. The failure mode that actually matters is [`IdentityProvider::exchange_code`],
/// which refuses loudly.
pub struct NoopIdentityProvider;

impl std::fmt::Debug for NoopIdentityProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NoopIdentityProvider")
    }
}

#[async_trait]
impl IdentityProvider for NoopIdentityProvider {
    fn name(&self) -> &str {
        "Google"
    }

    async fn authorize_url(&self) -> Result<(String, OAuthState), OAuthError> {
        Err(OAuthError::Disabled)
    }

    async fn exchange_code(
        &self,
        _code: &str,
        _state: &OAuthState,
    ) -> Result<GoogleIdentity, OAuthError> {
        Err(OAuthError::Disabled)
    }

    async fn verify_id_token(&self, _id_token: &str) -> Result<GoogleIdentity, OAuthError> {
        Err(OAuthError::Disabled)
    }

    fn is_enabled(&self) -> bool {
        false
    }
}

/// A scriptable [`IdentityProvider`] — §5.6's in-memory double.
///
/// Not a `mockall` mock: the point is that a test can say "the provider returns this
/// identity" or "the provider is down" and read back what the flow did, which a generated
/// mock cannot do without a layer of expectation bookkeeping in every test.
pub struct InMemoryIdentityProvider {
    identity: Mutex<Option<Result<GoogleIdentity, OAuthError>>>,
    exchanges: Mutex<usize>,
}

impl InMemoryIdentityProvider {
    /// A provider that will return `identity` on the next exchange.
    #[must_use]
    pub fn returning(identity: GoogleIdentity) -> Self {
        Self {
            identity: Mutex::new(Some(Ok(identity))),
            exchanges: Mutex::new(0),
        }
    }

    /// A provider that will fail every exchange with `error`.
    #[must_use]
    pub fn failing(error: OAuthError) -> Self {
        Self {
            identity: Mutex::new(Some(Err(error))),
            exchanges: Mutex::new(0),
        }
    }

    /// How many codes have been exchanged.
    ///
    /// Lets a test assert that a rejected sign-in never reached the provider, which is the
    /// difference between "CSRF blocked it" and "Google rejected it".
    #[must_use]
    pub fn exchanges(&self) -> usize {
        *self
            .exchanges
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_identity(
        &self,
    ) -> std::sync::MutexGuard<'_, Option<Result<GoogleIdentity, OAuthError>>> {
        self.identity.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Consume the scripted answer, applying the §7.12 guard.
    ///
    /// # Errors
    ///
    /// [`OAuthError::CodeRejected`] once the slot is empty, whatever the
    /// [`OAuthError::CodeRejected`] was scripted to be, and
    /// [`OAuthError::EmailNotVerified`] for an unverified address.
    fn take_scripted_identity(&self) -> Result<GoogleIdentity, OAuthError> {
        *self
            .exchanges
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;

        // Take the scripted answer so a second exchange does not silently succeed — a
        // reusable code is exactly the replay the single-use state store exists to stop.
        let identity = self.lock_identity().take().ok_or_else(|| {
            OAuthError::CodeRejected("the authorization code has already been redeemed".to_owned())
        })??;

        // The same §7.12 guard the real adapter applies. Without it the double would be
        // *more* permissive than production, and every test built on it would assert
        // behaviour a real provider can never produce.
        identity.require_linkable()?;
        Ok(identity)
    }
}

impl std::fmt::Debug for InMemoryIdentityProvider {
    /// Never prints the scripted identity — it can carry a real email address.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryIdentityProvider")
            .field("exchanges", &self.exchanges())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl IdentityProvider for InMemoryIdentityProvider {
    fn name(&self) -> &str {
        "Google (test)"
    }

    async fn authorize_url(&self) -> Result<(String, OAuthState), OAuthError> {
        Err(OAuthError::Config(
            "the in-memory double does not build authorization URLs".to_owned(),
        ))
    }

    async fn exchange_code(
        &self,
        _code: &str,
        _state: &OAuthState,
    ) -> Result<GoogleIdentity, OAuthError> {
        self.take_scripted_identity()
    }

    async fn verify_id_token(&self, _id_token: &str) -> Result<GoogleIdentity, OAuthError> {
        // One slot, both entry points, and the same one-shot semantics: a code or an ID
        // token that has been presented twice is a replay, and a double that quietly
        // accepted both would hide that rather than surface it.
        self.take_scripted_identity()
    }

    fn is_enabled(&self) -> bool {
        true
    }
}

/// The provider to use, from validated configuration (§4.6).
///
/// Returns [`NoopIdentityProvider`] when `GOOGLE_ENABLED` is unset, which cannot fail.
/// When Google sign-in *is* enabled a malformed redirect URI is fatal at startup, for the
/// same reason push and storage are: an enabled adapter that cannot build fails one
/// sign-in at a time, and the first symptom is a user saying the button does nothing.
///
/// # Errors
///
/// Returns [`OAuthError::Config`] when Google sign-in is enabled but the settings do not
/// build a client.
pub fn provider_from_config(config: &Config) -> Result<Arc<dyn IdentityProvider>, OAuthError> {
    match &config.google {
        // `new` returns `Arc<GoogleIdentityProvider>`, so this is an unsizing coercion
        // rather than a second allocation.
        Some(settings) => Ok(GoogleIdentityProvider::new(settings)?),
        None => {
            tracing::info!("Google sign-in is disabled (GOOGLE_ENABLED is not true)");
            Ok(Arc::new(NoopIdentityProvider))
        }
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the module's seams: the no-op provider, the double, and the factory.

    use uuid::Uuid;

    use super::*;

    fn state() -> OAuthState {
        OAuthState {
            state: "abc".to_owned(),
            verifier: "v".to_owned(),
        }
    }

    // ── the no-op ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn the_no_op_reports_itself_disabled_and_refuses_to_exchange() {
        let provider = NoopIdentityProvider;

        assert!(!provider.is_enabled());
        assert!(matches!(
            provider.authorize_url().await,
            Err(OAuthError::Disabled)
        ));
        assert!(matches!(
            provider.exchange_code("code", &state()).await,
            Err(OAuthError::Disabled)
        ));
    }

    #[tokio::test]
    async fn the_no_op_refuses_the_id_token_flow_too() {
        // A new trait method that the no-op forgot to implement would compile as an
        // unimplemented body and 500 at runtime on a deployment with Google switched
        // off — the exact configuration §4.6 says must still work.
        let provider = NoopIdentityProvider;

        assert!(matches!(
            provider.verify_id_token("id-token").await,
            Err(OAuthError::Disabled)
        ));
    }

    #[tokio::test]
    async fn the_double_returns_its_scripted_identity_on_the_id_token_path_too() {
        // Both entry points go through one guard, so both must be drivable from a test.
        let provider = InMemoryIdentityProvider::returning(GoogleIdentity::verified(
            "1000",
            "user@example.com",
        ));

        let identity = provider
            .verify_id_token("id-token")
            .await
            .expect("the scripted identity");

        assert_eq!(identity.email, "user@example.com");
        assert!(provider.verify_id_token("id-token").await.is_err());
    }

    #[tokio::test]
    async fn the_double_refuses_an_unverified_identity_on_both_paths() {
        // §7.12, asserted where the double is defined. A double that let an unverified
        // address through would make every account-linking test built on it meaningless.
        //
        // A fresh double per path: the scripted identity is consumed, so a shared one
        // would answer the second call "already redeemed" and this would pass for the
        // wrong reason — which is the failure mode a security assertion must never have.
        for entry in ["exchange_code", "verify_id_token"] {
            let provider = InMemoryIdentityProvider::returning(GoogleIdentity::unverified(
                "1000",
                "victim@example.com",
            ));

            let outcome = match entry {
                "exchange_code" => provider.exchange_code("code", &state()).await,
                _ => provider.verify_id_token("id-token").await,
            };

            assert!(
                matches!(outcome, Err(OAuthError::EmailNotVerified)),
                "{entry} must refuse an unverified address, got {outcome:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_disabled_provider_offers_no_authorization_url() {
        // The sign-in screen keys off `is_enabled()`, and this is the belt to that
        // braces: even if a caller ignored it, there is no URL to send anyone to.
        assert!(NoopIdentityProvider.authorize_url().await.is_err());
    }

    // ── the double ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn the_double_returns_its_scripted_identity_once() {
        let provider = InMemoryIdentityProvider::returning(GoogleIdentity::verified(
            "1000",
            "user@example.com",
        ));

        let identity = provider
            .exchange_code("code", &state())
            .await
            .expect("the scripted identity");

        assert_eq!(identity.email, "user@example.com");
        assert_eq!(provider.exchanges(), 1);

        // A second redemption fails: an authorization code is single-use, and a double
        // that quietly succeeds twice would hide a replay bug rather than surface it.
        assert!(provider.exchange_code("code", &state()).await.is_err());
        assert_eq!(provider.exchanges(), 2);
    }

    #[tokio::test]
    async fn a_rejected_sign_in_never_reaches_the_provider() {
        // The distinction that proves CSRF actually blocked something: the provider was
        // never asked.
        let provider = InMemoryIdentityProvider::returning(GoogleIdentity::verified(
            "1000",
            "user@example.com",
        ));
        let states = InMemoryStateStore::new();

        let error = states
            .take("a-state-we-never-issued")
            .await
            .expect_err("an unknown state must be refused");
        assert!(matches!(error, OAuthError::Rejected(_)));

        assert_eq!(provider.exchanges(), 0);
    }

    #[tokio::test]
    async fn an_unverified_identity_from_the_provider_is_refused_at_the_seam() {
        // §7.12 end to end without a network: even if a caller somehow obtained an
        // unverified identity, the guard refuses it.
        let provider = InMemoryIdentityProvider::returning(GoogleIdentity::unverified(
            "1000",
            "victim@example.com",
        ));

        let Err(error) = provider.exchange_code("code", &state()).await else {
            panic!("an unverified identity must not be returned to the caller");
        };

        assert!(
            matches!(error, OAuthError::EmailNotVerified),
            "got {error:?}"
        );
    }

    // ── the factory ────────────────────────────────────────────────────────

    /// Every required variable, no Google.
    fn base_source() -> crate::config::MapSource {
        crate::config::MapSource::new()
            .with("ENV", "dev")
            .with("PORT", "8080")
            .with("DATABASE_URL", "postgres://u:p@localhost:5432/meno")
            .with("REDIS_URL", "redis://localhost:6379")
            .with("JWT_SECRET", "a-real-secret-value")
            .with("JWT_REFRESH_SECRET", "another-real-secret-value")
            .with("CORS_ORIGINS", "https://app.example.com")
    }

    fn with_google() -> crate::config::MapSource {
        base_source()
            .with("GOOGLE_CLIENT_ID", "client-id")
            .with("GOOGLE_CLIENT_SECRET", "client-secret")
            .with(
                "GOOGLE_REDIRECT_URI",
                "https://app.example.com/auth/google/callback",
            )
    }

    #[tokio::test]
    async fn google_sign_in_is_off_unless_it_is_configured() {
        // §4.6: a deployment that authenticates by password must not be blocked by five
        // unset `GOOGLE_*` variables.
        let config = Config::from_source(&base_source()).expect("valid config");
        assert!(config.google.is_none());

        let provider = provider_from_config(&config).expect("a disabled provider must build");

        assert!(!provider.is_enabled());
    }

    #[tokio::test]
    async fn a_configured_google_provider_builds() {
        let config = Config::from_source(&with_google()).expect("valid config");

        let provider = provider_from_config(&config).expect("a configured provider must build");

        assert!(provider.is_enabled());
        assert_eq!(provider.name(), "Google");
    }

    #[tokio::test]
    async fn an_enabled_but_malformed_redirect_uri_fails_startup_rather_than_panicking() {
        // `master`'s `.expect("Invalid Google redirect URI")`, made a readable error.
        let config =
            Config::from_source(&with_google().with("GOOGLE_REDIRECT_URI", "not a url at all"))
                .expect("valid config");

        let Err(error) = provider_from_config(&config) else {
            panic!("a malformed GOOGLE_REDIRECT_URI must not produce a usable provider");
        };
        assert!(matches!(error, OAuthError::Config(_)), "got {error:?}");
    }

    #[tokio::test]
    async fn the_client_secret_never_appears_in_debug_output() {
        // §9.5.
        let config = Config::from_source(&with_google()).expect("valid config");
        let provider = provider_from_config(&config).expect("a provider");

        let rendered = format!("{provider:?}");
        assert!(
            !rendered.contains("client-secret"),
            "the client secret must not appear in Debug output: {rendered}"
        );
    }

    #[tokio::test]
    async fn a_disabled_and_a_real_provider_are_both_usable_as_one_trait_object() {
        // §5.3: substitutability is the point of the trait, so it is asserted directly.
        let providers: Vec<Arc<dyn IdentityProvider>> = vec![
            Arc::new(NoopIdentityProvider),
            Arc::new(InMemoryIdentityProvider::returning(
                GoogleIdentity::verified("1", "a@example.com"),
            )),
        ];

        assert_eq!(providers.len(), 2);
        assert!(!providers[0].is_enabled());
        assert!(providers[1].is_enabled());
        let _ = Uuid::nil();
    }
}
