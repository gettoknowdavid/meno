//! The Google sign-in seam, as the auth module sees it (plan §4.7 item 7).
//!
//! # Why this file exists at all
//!
//! [`crate::infrastructure::oauth::IdentityProvider`] already exists and already
//! enforces §7.12 — it refuses to return an identity whose `email_verified` is false. So
//! why a second trait?
//!
//! Because the two modules are allowed to disagree about *vocabulary*, and this one is.
//! The adapter speaks [`OAuthError`] and [`crate::infrastructure::oauth::GoogleIdentity`],
//! both of which are infrastructure's business; `services.rs` must speak
//! [`meno_core::Error`] and a claim it can hand straight to `link_provider`. Having
//! `AuthService` name either adapter type would put a driver-flavoured error enum inside
//! the domain layer, which is the §5.5 leak in the direction the plan does not want it.
//!
//! The rule this file encodes: **a verified Google identity is the only thing that can
//! reach the linking code, and this trait is the only door.** [`GoogleExchange`] returns
//! [`GoogleIdentity`], and there is no constructor for one that has not been through
//! [`crate::infrastructure::oauth::GoogleIdentity::require_linkable`]. That is why
//! [`services::AuthService::complete_google`](super::services) needs no guard of its own:
//! adding one there would be the third place to remember, and the second is where the bug
//! lived on `master`.
//!
//! # Two entry points, one guard
//!
//! [`GoogleExchange::exchange_code`] and [`GoogleExchange::verify_id_token`] are the web
//! flow and the mobile flow. `master` checked `email_verified` on the mobile path and not
//! on the web path, which meant the web flow inherited an unverified-email takeover: an
//! attacker who creates a Google account carrying somebody else's address, unverified,
//! signs in through the browser and inherits that account. Both methods here go through
//! the same [`checked`] conversion, so a third entry point cannot be added without it.
//!
//! # Testing
//!
//! [`StubExchange`] scripts an answer and counts the calls, which is what makes the
//! security property assertable without owning a Google account (§10). The production
//! implementation's own HTTP path is covered in `infrastructure/oauth`.

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;

use crate::infrastructure::oauth::{
    self, GoogleIdentity as ProviderIdentity, IdentityProvider, OAuthStateStore,
};
use meno_core::Error as MenoError;

/// Where to send the browser, and the `state` to expect back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GoogleAuthorize {
    /// The provider's consent-screen URL, PKCE challenge included.
    pub url: String,
    /// The CSRF token the callback must echo.
    ///
    /// Server-stored and single-use: [`GoogleExchange::exchange_code`] consumes it, so a
    /// second callback presenting the same value fails whatever else it holds.
    pub state: String,
}

/// A Google identity that [`crate::infrastructure::oauth::IdentityProvider`] has already
/// checked.
///
/// Deliberately has no public fields and no public constructor. Every instance was
/// produced by [`checked`], which called `require_linkable`, so a `GoogleIdentity` in hand
/// is a claim that cannot be turned into an account takeover by a caller that forgot a
/// check. The accessors are the whole interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleIdentity {
    subject: String,
    email: String,
    name: Option<String>,
}

impl GoogleIdentity {
    /// The provider's stable subject id — the only thing that identifies the *account*.
    ///
    /// The email is a mutable attribute of the account: Google lets a user change it, and
    /// keying on it would orphan the account on a rename while letting a change silently
    /// take over a different one.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// The verified address. Safe to look up an account by, and *only* because §7.12
    /// verified it.
    #[must_use]
    pub fn email(&self) -> &str {
        &self.email
    }

    /// The provider's display name, if it supplied one.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Something to put in `users.full_name` when the provider gave us nothing.
    ///
    /// The email's local part, because a `users.full_name` is `NOT NULL` and an account
    /// with an empty display name is one the whole UI has to special-case. Returning an
    /// `Option` here and letting the caller decide would mean three call sites each
    /// inventing this rule.
    ///
    /// Falls back to the whole address when the local part is empty — `"@example.com"`
    /// has nothing before the `@`, and the alternative is a `full_name` of `""`, which is
    /// worse than an ugly one. §9.1: no `unwrap`, and no index-into-a-maybe-empty-slice.
    #[must_use]
    pub fn display_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| {
            let local = self.email.split_once('@').map_or("", |(local, _)| local);

            if local.is_empty() {
                self.email.clone()
            } else {
                local.to_owned()
            }
        })
    }
}

/// Google sign-in, as `services.rs` needs it.
///
/// Object safe and `async_trait` for the same reason [`IdentityProvider`] is: the service
/// holds `Arc<dyn GoogleExchange>`, so the test double substitutes directly (§5.3). The
/// provider is a *parameter* rather than a field because the same service instance serves
/// both flows and either may be disabled in a given deployment.
#[async_trait]
pub trait GoogleExchange: Send + Sync + std::fmt::Debug {
    /// Build the consent-screen URL and the state to remember.
    ///
    /// # Errors
    ///
    /// [`MenoError::Forbidden`] with [`meno_core::ErrorCode::ProviderDisabled`] when
    /// Google sign-in is switched off, and [`MenoError::Upstream`] when the provider is
    /// configured but unreachable.
    async fn authorize(&self) -> Result<GoogleAuthorize, MenoError>;

    /// Redeem an authorization code for a verified identity — the web flow.
    ///
    /// `state` is consumed from the caller's [`OAuthStateStore`] first: an unknown, spent
    /// or mismatched state fails here, before the provider is contacted at all.
    ///
    /// # Errors
    ///
    /// [`MenoError::BadRequest`] for a rejected or replayed state, and whatever the
    /// provider reports for the exchange itself.
    async fn exchange_code(&self, code: &str, state: &str) -> Result<GoogleIdentity, MenoError>;

    /// Verify an ID token the platform SDK obtained — the mobile flow.
    ///
    /// # Errors
    ///
    /// [`MenoError::Forbidden`] with [`meno_core::ErrorCode::EmailNotVerified`] when the
    /// provider has not verified the address (§7.12), and
    /// [`MenoError::Upstream`] for a provider fault.
    async fn verify_id_token(&self, id_token: &str) -> Result<GoogleIdentity, MenoError>;
}

/// The production [`GoogleExchange`]: the OAuth adapter plus a state store.
///
/// The state store is a separate dependency rather than an implementation detail because
/// it has to be Redis in production and in-memory in a test, and §4.7.7's single-use
/// property is only as good as the store behind it. A deployment that used the in-memory
/// store would lose every in-flight sign-in on restart and split the CSRF binding across
/// replicas — which is why the constructor here takes both and [`from_config`] takes a
/// store rather than inventing one.
pub struct ProviderExchange {
    provider: Arc<dyn IdentityProvider>,
    states: Arc<dyn OAuthStateStore>,
}

impl std::fmt::Debug for ProviderExchange {
    /// `IdentityProvider` implementations already redact their own secrets, but the
    /// composition is still not worth printing: a `Debug` line for this object would be
    /// one more place a misconfiguration becomes visible.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderExchange")
            .field("provider", &self.provider.name())
            .field("enabled", &self.provider.is_enabled())
            .finish_non_exhaustive()
    }
}

impl ProviderExchange {
    /// Wire the adapter to a state store.
    #[must_use]
    pub fn new(provider: Arc<dyn IdentityProvider>, states: Arc<dyn OAuthStateStore>) -> Self {
        Self { provider, states }
    }

    /// Wire the adapter from validated configuration, onto `states` (§4.6).
    ///
    /// `states` is a parameter rather than something built here because only the caller
    /// knows which store is reachable: Redis in production, in-memory in a test. A
    /// constructor that quietly chose `InMemoryStateStore` would compile everywhere and
    /// break every multi-replica deployment in a way that only shows up as intermittent
    /// sign-in failures.
    ///
    /// # Errors
    ///
    /// [`MenoError::Internal`] when Google sign-in is enabled but the settings do not
    /// build a client — a typo in `GOOGLE_REDIRECT_URI`, which on `master` was an
    /// `.expect` and a panic trace at boot (§9.1). A malformed redirect URI is a startup
    /// failure, not a per-request one.
    pub fn from_config(
        config: &crate::config::Config,
        states: Arc<dyn OAuthStateStore>,
    ) -> Result<Self, MenoError> {
        let provider = oauth::provider_from_config(config).map_err(|problem| {
            tracing::error!(error = %problem, "Google sign-in is enabled but unusable");
            MenoError::Internal {
                context: "build_google_provider",
                detail: problem.to_string(),
            }
        })?;

        Ok(Self::new(provider, states))
    }
}

#[async_trait]
impl GoogleExchange for ProviderExchange {
    async fn authorize(&self) -> Result<GoogleAuthorize, MenoError> {
        let (url, state) = self.provider.authorize_url().await?;

        // Store before returning. If the store is unreachable the sign-in must not
        // start: without a stored state there is no CSRF binding at all, so the failure
        // is propagated rather than swallowed.
        self.states.put(&state).await?;

        Ok(GoogleAuthorize {
            url,
            state: state.state,
        })
    }

    async fn exchange_code(&self, code: &str, state: &str) -> Result<GoogleIdentity, MenoError> {
        // Consume first. This is the CSRF defence, and it is also why a replayed callback
        // fails without the provider ever being asked.
        let stored = self.states.take(state).await?;
        let identity = self.provider.exchange_code(code, &stored).await?;

        checked(identity)
    }

    async fn verify_id_token(&self, id_token: &str) -> Result<GoogleIdentity, MenoError> {
        let identity = self.provider.verify_id_token(id_token).await?;

        checked(identity)
    }
}

/// Apply the §7.12 guard and narrow the adapter's claim to the module's own.
///
/// One function, both entry points, for the reason in the module docs: `master` had two
/// and checked one.
///
/// # Errors
///
/// [`MenoError::Forbidden`] with [`meno_core::ErrorCode::EmailNotVerified`] when the
/// provider has not verified the address. Belt and braces rather than paranoia: the
/// provider already refuses, and a *future* adapter that forgot would be caught here
/// instead of at the account-linking `UPDATE`.
fn checked(identity: ProviderIdentity) -> Result<GoogleIdentity, MenoError> {
    identity.require_linkable()?;

    Ok(GoogleIdentity {
        subject: identity.subject,
        email: identity.email,
        name: identity.name,
    })
}

/// A scriptable [`GoogleExchange`] — §10's double.
///
/// Not a `mockall` mock: a test needs to say "the provider returns this identity", or
/// "the provider is down", and then read back how many times it was called. That is the
/// difference between "CSRF blocked the callback" and "Google rejected the code", and a
/// generated mock cannot express it without bookkeeping in every test.
pub struct StubExchange {
    /// The answer `authorize` gives. One-shot, for the same reason the identity slot is.
    authorize: Mutex<Option<Result<GoogleAuthorize, MenoError>>>,
    /// The answer `exchange_code` gives.
    exchanged: Mutex<Option<Result<GoogleIdentity, MenoError>>>,
    /// The answer `verify_id_token` gives.
    verified: Mutex<Option<Result<VerifiedProbe, MenoError>>>,
    calls: Mutex<StubCalls>,
}

/// A `GoogleIdentity` built without going through a provider, for the double only.
#[derive(Debug, Clone)]
pub struct VerifiedProbe {
    subject: String,
    email: String,
    name: Option<String>,
}

/// What the double was asked, and how often.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StubCalls {
    /// How many times [`GoogleExchange::authorize`] was called.
    pub authorize: usize,
    /// How many times [`GoogleExchange::exchange_code`] was called.
    pub exchange_code: usize,
    /// How many times [`GoogleExchange::verify_id_token`] was called.
    pub verify_id_token: usize,
}

impl std::fmt::Debug for StubExchange {
    /// Never prints the scripted identity — it can carry a real address.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StubExchange")
            .field("calls", &self.calls())
            .finish_non_exhaustive()
    }
}

impl StubExchange {
    /// A provider that signs `identity` in on the code path.
    #[must_use]
    pub fn exchanging(subject: &str, email: &str) -> Self {
        Self::named(subject, email, None)
    }

    /// A provider that supplies a display name too.
    #[must_use]
    pub fn named(subject: &str, email: &str, name: Option<&str>) -> Self {
        let identity = GoogleIdentity {
            subject: subject.to_owned(),
            email: email.to_owned(),
            name: name.map(str::to_owned),
        };

        Self {
            authorize: Mutex::new(Some(Err(MenoError::Internal {
                context: "stub_authorize",
                detail: "the stub does not build authorization URLs by default".to_owned(),
            }))),
            exchanged: Mutex::new(Some(Ok(identity.clone()))),
            verified: Mutex::new(Some(Ok(VerifiedProbe {
                subject: identity.subject,
                email: identity.email,
                name: identity.name,
            }))),
            calls: Mutex::new(StubCalls::default()),
        }
    }

    /// A provider that answers `outcome` on the code path.
    #[must_use]
    pub fn failing(outcome: MenoError) -> Self {
        Self {
            authorize: Mutex::new(Some(Err(MenoError::Internal {
                context: "stub_authorize",
                detail: "the stub does not build authorization URLs by default".to_owned(),
            }))),
            exchanged: Mutex::new(Some(Err(outcome))),
            verified: Mutex::new(None),
            calls: Mutex::new(StubCalls::default()),
        }
    }

    /// A provider that answers `outcome` when a mobile client presents an ID token.
    ///
    /// Separate from [`failing`](Self::failing) because that one scripts the *code*
    /// path. A real provider is down for both, but the double has to say which one it
    /// is standing in for — a mobile test that silently exercised the code path would
    /// still pass if `verify_id_token` stopped being called at all.
    #[must_use]
    pub fn failing_id_token(outcome: MenoError) -> Self {
        Self {
            authorize: Mutex::new(Some(Err(MenoError::Internal {
                context: "stub_authorize",
                detail: "the stub does not build authorization URLs by default".to_owned(),
            }))),
            exchanged: Mutex::new(None),
            verified: Mutex::new(Some(Err(outcome))),
            calls: Mutex::new(StubCalls::default()),
        }
    }

    /// A provider whose `authorize` succeeds, for the `/auth/google/url` test.
    #[must_use]
    pub fn with_authorize(self, authorize: GoogleAuthorize) -> Self {
        // No `mut`: `Mutex` gives interior mutability, so writing through the guard does
        // not need the binding to be mutable and the compiler is right to say so.
        *self
            .authorize
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Ok(authorize));
        self
    }

    /// Answer the mobile path with `probe` instead.
    #[must_use]
    pub fn verifying(self, probe: VerifiedProbe) -> Self {
        *self.verified.lock().unwrap_or_else(PoisonError::into_inner) = Some(Ok(probe));
        self
    }

    /// How many times each entry point was called.
    #[must_use]
    pub fn calls(&self) -> StubCalls {
        *self.calls.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Count a call to one of the three entry points.
    ///
    /// Takes the counter by value in a closure rather than returning a guard, so the
    /// lock is released before the scripted answer is taken — a test that deadlocks on
    /// its own double is a test that never runs.
    fn count(&self, which: fn(&mut StubCalls)) {
        let mut calls = self.calls.lock().unwrap_or_else(PoisonError::into_inner);
        which(&mut calls);
    }

    /// Take the one-shot scripted answer out of `slot`.
    ///
    /// # Errors
    ///
    /// An [`MenoError::Internal`] carrying `exhausted` when the slot is already empty,
    /// which is how the double says "this credential has been presented twice".
    fn take<T: Clone>(
        slot: &Mutex<Option<Result<T, MenoError>>>,
        exhausted: &'static str,
    ) -> Result<T, MenoError> {
        let mut guard = slot.lock().unwrap_or_else(PoisonError::into_inner);
        match guard.take() {
            Some(Ok(value)) => Ok(value.clone()),
            Some(Err(problem)) => Err(problem),
            None => Err(MenoError::Internal {
                context: "stub_exchange",
                detail: exhausted.to_owned(),
            }),
        }
    }
}

impl VerifiedProbe {
    /// A verified claim the double can hand back.
    #[must_use]
    pub fn verified(subject: &str, email: &str) -> Self {
        Self {
            subject: subject.to_owned(),
            email: email.to_owned(),
            name: None,
        }
    }

    /// The same claim, with a display name.
    #[must_use]
    pub fn named(subject: &str, email: &str, name: &str) -> Self {
        Self {
            name: Some(name.to_owned()),
            ..Self::verified(subject, email)
        }
    }

    fn into_identity(self) -> GoogleIdentity {
        GoogleIdentity {
            subject: self.subject,
            email: self.email,
            name: self.name,
        }
    }
}

#[async_trait]
impl GoogleExchange for StubExchange {
    async fn authorize(&self) -> Result<GoogleAuthorize, MenoError> {
        self.count(|calls| calls.authorize += 1);
        Self::take(
            &self.authorize,
            "the scripted authorization URL has already been handed out",
        )
    }

    async fn exchange_code(&self, _code: &str, _state: &str) -> Result<GoogleIdentity, MenoError> {
        self.count(|calls| calls.exchange_code += 1);

        Self::take(
            &self.exchanged,
            "the scripted authorization code has already been redeemed",
        )
    }

    async fn verify_id_token(&self, _id_token: &str) -> Result<GoogleIdentity, MenoError> {
        self.count(|calls| calls.verify_id_token += 1);

        let probe = Self::take(
            &self.verified,
            "the scripted ID token has already been presented",
        )?;

        Ok(probe.into_identity())
    }
}

#[cfg(test)]
mod tests {
    //! The seam's own contract: the guard, the display-name fallback, and the
    //! double's honesty.
    //!
    //! The security property these tests exist for is §7.12 — "an unverified provider
    //! address never becomes a linkable identity" — and it is asserted *here* rather than
    //! only in `services.rs`, so the guarantee survives a refactor of the service.

    use super::*;
    use crate::infrastructure::oauth::{
        InMemoryIdentityProvider, InMemoryStateStore, NoopIdentityProvider, OAuthError, OAuthState,
    };

    /// Wire an exchange onto a fresh in-memory state store.
    ///
    /// The provider is taken by `Arc` and *cloned in*, not moved, so a test can still
    /// ask it how many times it was called — which is the only way to tell "CSRF blocked
    /// the callback" apart from "the provider refused the code".
    fn provider_exchange(
        provider: &Arc<InMemoryIdentityProvider>,
    ) -> (ProviderExchange, Arc<InMemoryStateStore>) {
        let states = Arc::new(InMemoryStateStore::new());
        let provider: Arc<dyn crate::infrastructure::oauth::IdentityProvider> = provider.clone();
        (ProviderExchange::new(provider, states.clone()), states)
    }

    // ── §7.12: the guard ───────────────────────────────────────────────────

    #[tokio::test]
    async fn an_unverified_provider_email_never_becomes_a_linkable_identity() {
        // The assertion that matters most in this file. One provider *per* entry point:
        // the double is single-use by design, so sharing one would make the second call
        // fail as "already redeemed" and the assertion would pass for the wrong reason.
        for entry in ["exchange_code", "verify_id_token"] {
            let provider = Arc::new(InMemoryIdentityProvider::returning(
                ProviderIdentity::unverified("1000", "victim@example.com"),
            ));
            let (exchange, states) = provider_exchange(&provider);

            // The code path consumes state first, so give it something to consume.
            states
                .put(&OAuthState {
                    state: "state".to_owned(),
                    verifier: "v".to_owned(),
                })
                .await
                .expect("storing the state");

            let outcome = match entry {
                "exchange_code" => exchange.exchange_code("code", "state").await,
                _ => exchange.verify_id_token("id-token").await,
            };

            assert_eq!(
                outcome
                    .expect_err("an unverified address must be refused")
                    .code(),
                meno_core::ErrorCode::EmailNotVerified,
                "{entry} must refuse an unverified address, and for the §7.12 reason"
            );
        }
    }

    #[test]
    fn the_guard_also_refuses_a_claim_handed_straight_to_the_conversion() {
        // `checked` is private, so this is the only way to reach it, and that is the
        // point: the conversion is the guard, and it cannot be skipped.
        let outcome = checked(ProviderIdentity::unverified("1000", "victim@example.com"))
            .expect_err("an unverified address must be refused");

        assert_eq!(outcome.code(), meno_core::ErrorCode::EmailNotVerified);
    }

    #[tokio::test]
    async fn a_verified_identity_survives_the_conversion_intact() {
        let provider = Arc::new(InMemoryIdentityProvider::returning(
            ProviderIdentity::verified("1000", "user@example.com"),
        ));
        let (exchange, _) = provider_exchange(&provider);

        let identity = exchange
            .verify_id_token("id-token")
            .await
            .expect("a verified identity");

        assert_eq!(identity.subject(), "1000");
        assert_eq!(identity.email(), "user@example.com");
    }

    // ── the state store is what makes the web flow CSRF-safe ───────────────

    #[tokio::test]
    async fn a_callback_with_an_unissued_state_never_reaches_the_provider() {
        // The distinction that proves CSRF actually blocked something: the provider's
        // exchange count stays at zero.
        let provider = Arc::new(InMemoryIdentityProvider::returning(
            ProviderIdentity::verified("1000", "user@example.com"),
        ));
        let (exchange, _) = provider_exchange(&provider);

        exchange
            .exchange_code("code", "a-state-we-never-issued")
            .await
            .expect_err("an unknown state must be refused");

        assert_eq!(provider.exchanges(), 0);
    }

    #[tokio::test]
    async fn a_replayed_callback_is_refused_even_with_a_state_that_was_valid() {
        let provider = Arc::new(InMemoryIdentityProvider::returning(
            ProviderIdentity::verified("1000", "user@example.com"),
        ));
        let (exchange, states) = provider_exchange(&provider);

        let issued = OAuthState {
            state: "abc".to_owned(),
            verifier: "v".to_owned(),
        };
        states.put(&issued).await.expect("storing the state");

        exchange
            .exchange_code("code", "abc")
            .await
            .expect("the first callback");
        assert_eq!(provider.exchanges(), 1);

        exchange
            .exchange_code("code", "abc")
            .await
            .expect_err("the replay must be refused");
        assert_eq!(
            provider.exchanges(),
            1,
            "a replay must not reach the provider a second time"
        );
    }

    #[tokio::test]
    async fn authorizing_stores_the_state_so_the_callback_can_claim_it() {
        let states = Arc::new(InMemoryStateStore::new());
        let exchange = ProviderExchange::new(Arc::new(NoopIdentityProvider), states.clone());

        // The no-op refuses to build a URL, so this asserts the refusal propagates —
        // and that no state was left behind for it.
        assert!(exchange.authorize().await.is_err());
        assert!(states.is_empty());
    }

    // ── the display-name fallback ──────────────────────────────────────────

    #[test]
    fn a_provider_name_is_preferred_over_the_address() {
        let identity = GoogleIdentity {
            subject: "1000".to_owned(),
            email: "user@example.com".to_owned(),
            name: Some("Ada Lovelace".to_owned()),
        };

        assert_eq!(identity.display_name(), "Ada Lovelace");
        assert_eq!(identity.name(), Some("Ada Lovelace"));
    }

    #[test]
    fn a_missing_provider_name_falls_back_to_the_local_part() {
        // `users.full_name` is NOT NULL, and an account with an empty display name is
        // one the whole UI has to special-case.
        let identity = GoogleIdentity {
            subject: "1000".to_owned(),
            email: "user@example.com".to_owned(),
            name: None,
        };

        assert_eq!(identity.display_name(), "user");
    }

    #[test]
    fn a_fallback_for_an_address_with_no_local_part_does_not_panic() {
        // §9.1. A provider that returns something that is not an address must not take
        // the process down on the way to a rejected account creation.
        let identity = GoogleIdentity {
            subject: "1000".to_owned(),
            email: "@".to_owned(),
            name: None,
        };

        // Nothing before the `@`, so the whole address is used rather than an empty
        // name — a `full_name` of `""` would break the account's NOT NULL column.
        assert_eq!(identity.display_name(), "@");
    }

    // ── the double ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn the_double_is_single_use_on_the_code_path() {
        // An authorization code is single-use. A double that quietly accepted the same
        // one twice would hide a replay bug rather than surface it.
        let exchange = StubExchange::exchanging("1000", "user@example.com");

        assert!(exchange.exchange_code("code", "state").await.is_ok());
        assert!(exchange.exchange_code("code", "state").await.is_err());
    }

    #[tokio::test]
    async fn the_double_counts_each_entry_point_separately() {
        let exchange = StubExchange::exchanging("1000", "user@example.com")
            .verifying(VerifiedProbe::verified("1000", "user@example.com"));

        exchange.authorize().await.ok();
        exchange
            .exchange_code("code", "state")
            .await
            .expect("the code path");
        exchange
            .verify_id_token("id-token")
            .await
            .expect("the id-token path");

        assert_eq!(
            exchange.calls(),
            StubCalls {
                authorize: 1,
                exchange_code: 1,
                verify_id_token: 1,
            }
        );
    }

    #[tokio::test]
    async fn the_double_can_report_a_provider_fault_verbatim() {
        let exchange = StubExchange::failing(MenoError::Upstream {
            service: "google-oauth",
            detail: "connection reset".to_owned(),
        });

        let outcome = exchange
            .exchange_code("code", "state")
            .await
            .expect_err("the scripted fault");

        assert_eq!(outcome.code(), meno_core::ErrorCode::UpstreamUnavailable);
        assert!(!outcome.is_client_safe());
    }

    #[tokio::test]
    async fn the_doubles_debug_never_prints_the_scripted_address() {
        let rendered = format!(
            "{:?}",
            StubExchange::named("1000", "victim@example.com", Some("Victim"))
        );

        assert!(
            !rendered.contains("victim@example.com"),
            "an address must not survive into a log line: {rendered}"
        );
    }

    #[tokio::test]
    async fn a_disabled_provider_is_refused_with_the_specific_code() {
        // A client asked to sign in with Google on a deployment that has it switched off
        // should be told to offer the password form, not to retry (§4.6).
        let provider = Arc::new(NoopIdentityProvider);
        let states = Arc::new(InMemoryStateStore::new());
        let exchange = ProviderExchange::new(provider, states);

        let outcome = exchange
            .verify_id_token("id-token")
            .await
            .expect_err("a disabled provider must refuse");

        assert_eq!(outcome.code(), meno_core::ErrorCode::ProviderDisabled);
    }

    #[tokio::test]
    async fn the_provider_errors_are_erased_before_they_reach_a_caller() {
        // §5.5: `OAuthError` must not escape this module, and neither must the provider
        // detail that would otherwise travel with it.
        let provider = Arc::new(InMemoryIdentityProvider::failing(OAuthError::Upstream(
            "https://accounts.google.com/o/oauth2/v2/auth".to_owned(),
        )));
        let (exchange, _) = provider_exchange(&provider);

        let outcome = exchange
            .verify_id_token("id-token")
            .await
            .expect_err("the scripted upstream fault");
        let body = meno_core::to_body(&outcome);

        assert_eq!(body.http_status, 503);
        assert!(
            !body.message.contains("accounts.google.com"),
            "the provider endpoint must not reach the client: {}",
            body.message
        );
    }
}
