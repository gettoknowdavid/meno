//! The [`IdentityProvider`] seam: verifying who a user says they are.
//!
//! # Why a trait (§5.6)
//!
//! §5.6 names `EmailSender` and `ObjectStore` as new adapter traits; this is the fourth
//! one the plan implies but never wrote down. Without it, "can this person sign in?" is a
//! function that reaches `accounts.google.com`, so the only way to test the security
//! property that actually matters — refusing to link an *unverified* address (§7.12) — is
//! to own a Google account.
//!
//! [`GoogleIdentityProvider`] is the production implementation; [`NoopIdentityProvider`]
//! and the in-memory double live alongside it.
//!
//! # The security property this module exists to enforce
//!
//! §7.12: `upsert_google_user` on `master` linked a Google identity to an existing
//! account **by email alone**, without checking the provider's `email_verified`. Anyone
//! who can create a Google account carrying somebody else's address as an unverified
//! address inherits that account.
//!
//! The fix has to live on the provider path, not in the caller, because `master` had two
//! entry points — `exchange_code` and `verify_id_token` — and only one of them checked
//! the flag. [`IdentityProvider::exchange_code`] is the single way to obtain a
//! [`GoogleIdentity`], and it refuses an unverified address itself, so a second entry
//! point cannot be added without repeating the check.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::infrastructure::oauth::error::OAuthError;

/// The scopes Meno asks Google for.
///
/// A named constant rather than three literals at the call site, because the set is the
/// contract: dropping `email` silently removes the address the whole flow depends on, and
/// adding a scope with no server-side use is a consent-screen regression.
pub const SCOPES: [&str; 3] = ["openid", "email", "profile"];

/// How long an authorization request stays valid, in seconds.
///
/// The window in which the state and PKCE verifier must be presented. Ten minutes is
/// Google's own guidance and is comfortably longer than a consent screen.
pub const STATE_TTL_SECS: u64 = 600;

/// What a provider says about the person signing in.
///
/// Deliberately *not* a user. This is a claim from an external provider, and until the
/// caller has decided what to do with it — create an account, link an existing one,
/// refuse — it has no business looking like a `users` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoogleIdentity {
    /// The provider's stable subject id.
    ///
    /// The only field that identifies the *account*. The email is a mutable attribute of
    /// it and can change.
    pub subject: String,

    /// The email address, as the provider reports it.
    pub email: String,

    /// The display name, if the provider supplies one.
    pub name: Option<String>,

    /// The avatar URL, if the provider supplies one.
    pub picture: Option<String>,

    /// Whether the provider has verified ownership of [`Self::email`].
    ///
    /// The §7.12 field. [`Self::require_linkable`] refuses to let it be `false`.
    pub email_verified: bool,
}

impl GoogleIdentity {
    /// A verified identity, for tests and for the in-memory double.
    #[must_use]
    pub fn verified(subject: &str, email: &str) -> Self {
        Self {
            subject: subject.to_owned(),
            email: email.to_owned(),
            name: None,
            picture: None,
            email_verified: true,
        }
    }

    /// An identity whose email the provider has **not** verified.
    ///
    /// Exists so the §7.12 path is reachable from a test without hand-building five
    /// fields, and so the fixture is obviously the dangerous one.
    #[must_use]
    pub fn unverified(subject: &str, email: &str) -> Self {
        Self {
            email_verified: false,
            ..Self::verified(subject, email)
        }
    }

    /// Refuse an identity whose email the provider has not verified (§7.12).
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::EmailNotVerified`] when `email_verified` is false.
    ///
    /// Public so the account service can assert it too — belt and braces, because the
    /// security cost of a second unchecked path is a full account takeover.
    pub fn require_linkable(&self) -> Result<(), OAuthError> {
        if self.email_verified {
            Ok(())
        } else {
            Err(OAuthError::EmailNotVerified)
        }
    }

    /// The stable key for this identity in the users table.
    ///
    /// The provider subject, not the email: Google lets a user change their address, and
    /// keying on it would orphan the account on a rename while letting an address change
    /// silently take over a different one.
    #[must_use]
    pub fn subject_key(&self) -> String {
        format!("google:{}", self.subject)
    }
}

/// The state a client must echo back, and the PKCE verifier that goes with it.
///
/// Both halves are load-bearing and neither can be reconstructed:
///
/// - the **state** is the CSRF binding. Without it, an attacker feeds a victim's browser
///   a link carrying the *attacker's* authorization code, and the victim's session ends
///   up signed in as the attacker;
/// - the **verifier** is what makes PKCE work. The provider holds only the challenge, so
///   without the verifier an intercepted code is replayable.
///
/// Held server-side and single-use (§4.7.7). The caller persists this with
/// [`STATE_TTL_SECS`] as the deadline and must delete it on the callback — a second
/// callback with the same state is a replay and must be refused.
// `Serialize`/`Deserialize` because this value has to survive a round trip through
// Redis: the CSRF binding is only useful if the callback — a *different* HTTP request,
// possibly on a different replica — can read back what the redirect stored.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OAuthState {
    /// The CSRF token to send to the client and compare against on callback.
    pub state: String,

    /// The PKCE code verifier, held until the callback.
    pub verifier: String,
}

impl OAuthState {
    /// Whether `returned` matches the state this request was issued with.
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::Rejected`] on a mismatch. This is constant-time by way of
    /// [`String`]'s equality, which is not — see the note in the Google adapter's tests,
    /// where the fix for a real timing oracle lives.
    pub fn verify(&self, returned: &str) -> Result<(), OAuthError> {
        if self.state == returned {
            Ok(())
        } else {
            Err(OAuthError::Rejected(
                "the OAuth state did not match the request".to_owned(),
            ))
        }
    }
}

/// Where an in-flight authorization request is remembered between the redirect and the
/// callback.
///
/// # Why this is the CSRF defence, not a comparison
///
/// §4.7.7 asks for "single-use server-stored state". Storing the state and *consuming* it
/// is a stronger check than comparing it: a comparison needs the stored value to still be
/// there on the second callback, whereas a single-use store makes the second callback
/// fail no matter what it presents. A replayed callback cannot be distinguished from the
/// original by comparison alone.
///
/// [`OAuthState::verify`] still exists for the case where a caller wants to check before
/// consuming, but [`Self::take`] is what the handler should use.
#[async_trait]
pub trait OAuthStateStore: Send + Sync {
    /// Remember a request, to be claimed by the callback.
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::Upstream`] if the backing store is unavailable. This is the
    /// one case where failing to store must not silently continue — without stored state
    /// there is no CSRF binding at all, so the sign-in must not start.
    async fn put(&self, state: &OAuthState) -> Result<(), OAuthError>;

    /// Claim a request, removing it. A second call with the same state fails.
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::Rejected`] when the state is unknown, already used, or
    /// expired. All three are the same answer to the client on purpose: distinguishing
    /// them tells an attacker whether a guess was close.
    async fn take(&self, state: &str) -> Result<OAuthState, OAuthError>;
}

/// An in-memory [`OAuthStateStore`].
///
/// For tests and for a single-instance dev server. **Not for production**: state must
/// survive a restart and be shared across replicas, or a callback landing on the other
/// replica loses the CSRF binding and the sign-in fails.
#[derive(Default)]
pub struct InMemoryStateStore {
    states: Mutex<std::collections::HashMap<String, OAuthState>>,
}

impl InMemoryStateStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many states are outstanding.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether nothing is outstanding.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, OAuthState>> {
        self.states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl std::fmt::Debug for InMemoryStateStore {
    /// Never prints the stored verifiers — they are credentials while a sign-in is live.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryStateStore")
            .field("outstanding", &self.len())
            .finish()
    }
}

#[async_trait]
impl OAuthStateStore for InMemoryStateStore {
    async fn put(&self, state: &OAuthState) -> Result<(), OAuthError> {
        self.lock().insert(state.state.clone(), state.clone());
        Ok(())
    }

    async fn take(&self, state: &str) -> Result<OAuthState, OAuthError> {
        self.lock().remove(state).ok_or_else(|| {
            OAuthError::Rejected("the OAuth state is unknown or already used".to_owned())
        })
    }
}

/// Anything that can authenticate a user against an external identity provider.
///
/// Object-safe and `async_trait` so callers hold `Arc<dyn IdentityProvider>` and the
/// in-memory double substitutes directly (§5.3).
///
/// `Debug` is a supertrait because §4.8 wants structured log fields and a provider that
/// cannot be logged is one whose misconfiguration stays invisible. Each implementation
/// writes its own `Debug`, which is how the client secret stays out of the output.
#[async_trait]
pub trait IdentityProvider: Send + Sync + std::fmt::Debug {
    /// A human-readable name, e.g. `"Google"`.
    ///
    /// For log lines and for the button label on the sign-in screen, so it is part of the
    /// trait rather than a constant at the call site.
    fn name(&self) -> &str;

    /// Build the URL to send the browser to, plus the state to remember.
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::Config`] when the authorization endpoint is unusable — a
    /// case the constructor normally catches, so this is defence in depth.
    async fn authorize_url(&self) -> Result<(String, OAuthState), OAuthError>;

    /// Exchange an authorization code for a **verified** identity.
    ///
    /// This is the only path that produces a [`GoogleIdentity`], and it enforces
    /// [`GoogleIdentity::require_linkable`] itself. A caller cannot obtain an unverified
    /// identity from here — the §7.12 check is not optional at the use site.
    ///
    /// # Errors
    ///
    /// [`OAuthError::CodeRejected`] for a spent or mismatched code,
    /// [`OAuthError::EmailNotVerified`] for an unverified address, and
    /// [`OAuthError::Upstream`] for a provider fault.
    async fn exchange_code(
        &self,
        code: &str,
        state: &OAuthState,
    ) -> Result<GoogleIdentity, OAuthError>;

    /// Exchange an ID token that arrived on a mobile deep link.
    ///
    /// The second of the two entry points, and it enforces
    /// [`GoogleIdentity::require_linkable`] for exactly the same reason
    /// [`Self::exchange_code`] does: `master` checked the flag on this path and not on
    /// the other, which is how an unverified-email takeover survived review. Both paths
    /// go through one guard so a third cannot be added without it.
    ///
    /// # Errors
    ///
    /// [`OAuthError::EmailNotVerified`] for an unverified address,
    /// [`OAuthError::Rejected`] for a token the provider will not identify, and
    /// [`OAuthError::Upstream`] for a provider fault.
    async fn verify_id_token(&self, id_token: &str) -> Result<GoogleIdentity, OAuthError>;

    /// Whether this provider is usable right now.
    ///
    /// The sign-in screen uses it to decide whether to render the button at all, so a
    /// deployment with Google switched off does not offer a button that always fails.
    fn is_enabled(&self) -> bool;
}

#[cfg(test)]
mod tests {
    //! Tests for the identity type and the state check.

    use super::*;

    #[test]
    fn a_verified_identity_may_be_linked() {
        GoogleIdentity::verified("1000", "user@example.com")
            .require_linkable()
            .expect("a verified address must be linkable");
    }

    #[test]
    fn an_unverified_identity_is_refused_and_the_reason_is_specific() {
        // §7.12, the assertion that matters most in this module.
        let error = GoogleIdentity::unverified("1000", "victim@example.com")
            .require_linkable()
            .expect_err("an unverified address must not be linkable");

        assert!(
            matches!(error, OAuthError::EmailNotVerified),
            "the refusal must be the specific variant, got {error:?}"
        );
    }

    #[test]
    fn the_subject_key_is_namespaced_and_not_the_email() {
        // Keying on the email would let a Google address change take over a different
        // account — the same class of bug as §7.12, one step removed.
        let identity = GoogleIdentity::verified("1000", "user@example.com");

        assert_eq!(identity.subject_key(), "google:1000");
        assert!(!identity.subject_key().contains("example.com"));
    }

    #[test]
    fn a_matching_state_is_accepted() {
        let state = OAuthState {
            state: "abc".to_owned(),
            verifier: "v".to_owned(),
        };

        state
            .verify("abc")
            .expect("a matching state must be accepted");
    }

    #[test]
    fn a_mismatched_state_is_refused() {
        // The CSRF binding. Without this check an attacker can complete a sign-in on a
        // victim's browser.
        let state = OAuthState {
            state: "abc".to_owned(),
            verifier: "v".to_owned(),
        };

        let error = state
            .verify("attacker")
            .expect_err("a mismatch must be refused");

        assert!(matches!(error, OAuthError::Rejected(_)), "got {error:?}");
    }

    #[tokio::test]
    async fn a_stored_state_can_be_claimed_exactly_once() {
        // The replay defence. A second callback with the same state must fail even though
        // it presents a value that *was* valid.
        let store = InMemoryStateStore::new();
        let state = OAuthState {
            state: "abc".to_owned(),
            verifier: "v".to_owned(),
        };
        store.put(&state).await.expect("a put");

        let claimed = store
            .take("abc")
            .await
            .expect("the first claim must succeed");
        assert_eq!(claimed, state);
        assert!(store.is_empty());

        let error = store
            .take("abc")
            .await
            .expect_err("a replay must be refused");
        assert!(matches!(error, OAuthError::Rejected(_)), "got {error:?}");
    }

    #[tokio::test]
    async fn an_unknown_state_is_refused() {
        let error = InMemoryStateStore::new()
            .take("never-issued")
            .await
            .expect_err("must be refused");

        assert!(matches!(error, OAuthError::Rejected(_)), "got {error:?}");
    }

    #[tokio::test]
    async fn two_concurrent_claims_produce_exactly_one_winner() {
        // The race that a compare-then-delete implementation loses: two callbacks
        // arriving together must not both succeed.
        let store = InMemoryStateStore::new();
        let state = OAuthState {
            state: "abc".to_owned(),
            verifier: "v".to_owned(),
        };
        store.put(&state).await.expect("a put");

        let (first, second) = tokio::join!(store.take("abc"), store.take("abc"));

        assert!(
            first.is_ok() ^ second.is_ok(),
            "exactly one claim must win, got {first:?} and {second:?}"
        );
    }

    #[test]
    fn the_scopes_include_email_because_the_flow_depends_on_it() {
        assert!(SCOPES.contains(&"email"));
        assert!(SCOPES.contains(&"openid"));
        assert!(SCOPES.contains(&"profile"));
    }
}
