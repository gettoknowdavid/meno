//! Google `OAuth2` access tokens for the FCM v1 API, and the cache in front of them.
//!
//! Split out of `apps/api/src/shared/services/push/mod.rs` on `master` (`903c3ba`),
//! where `get_access_token`, `fetch_google_token` and `build_service_account_jwt` were
//! private methods of the sender. They are here because they are a separate
//! responsibility with a separate failure mode (§5.1): a bad service-account key has
//! nothing to do with a notification that will not reach a device, and the two must be
//! diagnosable apart.
//!
//! # The flow
//!
//! FCM v1 takes a short-lived Google `OAuth2` access token, not the service-account key.
//! The key signs a JWT assertion; Google exchanges the assertion for an access token
//! with the `firebase.messaging` scope. So:
//!
//! ```text
//! service-account key ──sign RS256──▶ JWT assertion ──POST /token──▶ access token
//!        (once, at startup)              (per refresh)               (1 hour)
//! ```
//!
//! The assertion is rebuilt on every refresh — it carries an `iat` and an `exp`, so a
//! cached one would be rejected as expired. The signing key is parsed once, at
//! construction, so a malformed PEM is a startup failure instead of a per-notification
//! one.
//!
//! # The cache, and why the lock is held across the fetch
//!
//! [`AccessTokenProvider::bearer`] holds the mutex for the whole refresh. That serialises
//! concurrent sends behind one token request, which is the point: without it, a
//! fan-out that starts just after expiry fires N simultaneous token requests, all of
//! which Google rate-limits, and the notification fan-out fails for a reason that has
//! nothing to do with FCM. The cost is that the first send after expiry waits for the
//! others — paid once an hour, against a provider that is entitled to one request.
//!
//! Tokens are cached with a [`SKEW_SECS`] safety margin rather than to the second of
//! expiry: Google issues an access token with an `exp` measured on *its* clock, and
//! ours is not synchronised with it, so a token used in its final second is rejected.

use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::Client;
use serde::Serialize;
use time::OffsetDateTime;
use tokio::sync::Mutex;

use crate::infrastructure::push::dto::{GoogleTokenResponse, ServiceAccount};
use crate::infrastructure::push::error::PushError;

/// The scope a push token must be granted. Anything less and FCM rejects every send
/// with `403 PERMISSION_DENIED`, which reads exactly like bad credentials.
pub const MESSAGING_SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";

/// How long before Google's stated expiry a cached token is treated as stale.
///
/// Sixty seconds: comfortably more than any plausible clock skew between this process
/// and Google's, and small enough that a token is never spent on its last second.
pub const SKEW_SECS: u64 = 60;

/// How long an assertion is valid for.
///
/// Google allows an hour and rejects one with a longer `exp`, so this is the maximum
/// rather than a preference.
const ASSERTION_LIFETIME_SECS: i64 = 3600;

/// A cached access token and the instant it stops being usable.
///
/// `Instant` rather than `SystemTime`: this is a *local* freshness decision, and
/// `Instant` cannot move backwards when NTP adjusts the clock — a backwards system
/// clock would otherwise make a dead token look fresh for an hour.
#[derive(Clone, Debug)]
struct CachedToken {
    access_token: String,

    /// The local instant after which this token must not be used.
    expires_at: Instant,
}

impl CachedToken {
    /// Whether this token may still be used at `now`.
    ///
    /// Already carries the skew: `expires_at` was set to the true expiry *minus*
    /// [`SKEW_SECS`], so this is a plain comparison and no two callers can disagree
    /// about how much margin to leave.
    fn usable_at(&self, now: Instant) -> bool {
        self.expires_at > now
    }
}

/// How long Google's `expires_in` is worth caching for.
///
/// `saturating_sub` because a token Google claims expires in under a minute is not an
/// error worth failing a send over — it yields a zero-length lifetime, so the next send
/// simply fetches another one.
#[must_use]
pub fn usable_lifetime(expires_in: u64) -> Duration {
    Duration::from_secs(expires_in.saturating_sub(SKEW_SECS))
}

/// Mints and caches the bearer tokens the FCM sender needs.
#[derive(Clone)]
pub struct AccessTokenProvider {
    http: Client,
    account: Arc<ServiceAccount>,
    signing_key: EncodingKey,
    cache: Arc<Mutex<Option<CachedToken>>>,
}

impl fmt::Debug for AccessTokenProvider {
    /// Redacts the service-account key.
    ///
    /// Hand-written for the same reason as `LivekitService`'s: this type holds an
    /// RSA private key and will end up inside application state that gets logged.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccessTokenProvider")
            .field("client_email", &self.account.client_email)
            .field("token_uri", &self.account.token_uri())
            .field("private_key", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl AccessTokenProvider {
    /// Parse the key and pre-load the signing material.
    ///
    /// # Errors
    ///
    /// Returns [`PushError::TokenFetch`] if the account's `private_key` is not a
    /// usable RSA PEM. Deliberately eager: this is the one FCM failure that is
    /// guaranteed and permanent, so the only useful moment to report it is startup,
    /// where `bootstrap` can refuse to boot with a message naming the variable.
    pub fn new(http: Client, account: ServiceAccount) -> Result<Self, PushError> {
        let signing_key = EncodingKey::from_rsa_pem(account.private_key.expose().as_bytes())
            .map_err(|e| {
                PushError::TokenFetch(format!(
                    "service-account `private_key` is not a usable RSA key: {e}"
                ))
            })?;

        Ok(Self {
            http,
            account: Arc::new(account),
            signing_key,
            cache: Arc::new(Mutex::new(None)),
        })
    }

    /// The service account this provider signs as.
    #[must_use]
    pub fn account(&self) -> &ServiceAccount {
        &self.account
    }

    /// A bearer token to send as `Authorization: Bearer …`, refreshed when stale.
    ///
    /// # Errors
    ///
    /// Returns [`PushError::TokenFetch`] if no token could be obtained. Sends should
    /// treat that as "push is misconfigured", not "this device is unreachable" —
    /// [`PushError::is_retryable`] agrees.
    pub async fn bearer(&self) -> Result<String, PushError> {
        // Held across `fetch` deliberately; see the module docs. `MutexGuard` is not
        // `Send`, but the guard never crosses an await in the *caller* of this future,
        // and this future is awaited inside a `Send` task by the sender.
        let mut cache = self.cache.lock().await;
        let now = Instant::now();

        if let Some(cached) = cache.as_ref()
            && cached.usable_at(now)
        {
            return Ok(cached.access_token.clone());
        }

        let fetched = self.fetch().await?;
        let lifetime = usable_lifetime(fetched.expires_in);
        let access_token = fetched.access_token.clone();

        *cache = Some(CachedToken {
            access_token: fetched.access_token,
            expires_at: now + lifetime,
        });

        Ok(access_token)
    }

    /// Exchange a freshly-signed assertion for an access token.
    async fn fetch(&self) -> Result<GoogleTokenResponse, PushError> {
        let issued_at = OffsetDateTime::now_utc().unix_timestamp();
        let assertion = self.assertion(issued_at)?;

        let response = self
            .http
            .post(self.account.token_uri())
            .form(&[
                ("grant_type", "urn:ietf:params:oauth2:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .await
            .map_err(|e| PushError::Transport(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            // The body is Google's `{"error": ...}` and may quote back the assertion,
            // so it is deliberately not read. The status is the actionable part.
            return Err(PushError::TokenFetch(format!(
                "the token endpoint returned {}",
                status.as_u16()
            )));
        }

        response.json::<GoogleTokenResponse>().await.map_err(|e| {
            PushError::TokenFetch(format!("the token response could not be read: {e}"))
        })
    }

    /// Sign the RS256 assertion that Google exchanges for an access token.
    ///
    /// Split out from [`Self::fetch`] so the claims can be asserted directly, which is
    /// the only way to catch a wrong `aud` or a missing `scope` — both of which fail at
    /// Google's end with a `400` that says nothing about which claim was wrong.
    ///
    /// # Errors
    ///
    /// Returns [`PushError::TokenFetch`] if the assertion cannot be signed, which for
    /// a key that parsed at construction means the crypto backend itself failed.
    fn assertion(&self, issued_at: i64) -> Result<String, PushError> {
        let claims = JwtClaims {
            iss: &self.account.client_email,
            scope: MESSAGING_SCOPE,
            aud: self.account.token_uri(),
            iat: issued_at,
            exp: issued_at + ASSERTION_LIFETIME_SECS,
        };

        encode(&Header::new(Algorithm::RS256), &claims, &self.signing_key).map_err(|e| {
            PushError::TokenFetch(format!("the JWT assertion could not be signed: {e}"))
        })
    }
}

/// The claims Google checks in a service-account assertion.
///
/// `Serialize` only — this is signed, never verified here, and a deserialisable
/// version would suggest otherwise.
#[derive(Debug, Serialize)]
struct JwtClaims<'a> {
    /// The service account's email. Google rejects an assertion signed by anyone else.
    iss: &'a str,

    /// `firebase.messaging`, and nothing less.
    scope: &'a str,

    /// The token endpoint the assertion is being presented to.
    aud: &'a str,

    /// Issued-at, whole seconds.
    iat: i64,

    /// Expiry, whole seconds. Google's maximum for this grant is one hour.
    exp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::push::test_key;

    /// A provider whose key is real but whose endpoint is unreachable, for the tests
    /// that only exercise construction and the cache.
    fn provider() -> AccessTokenProvider {
        AccessTokenProvider::new(
            Client::new(),
            ServiceAccount::parse(&test_key::service_account_json("http://127.0.0.1:1/token"))
                .expect("the test key parses"),
        )
        .expect("the test key is a usable RSA key")
    }

    #[tokio::test]
    async fn an_unusable_private_key_is_rejected_at_construction() {
        // The one guaranteed, permanent failure in the whole adapter. Reporting it at
        // startup beats discovering it from a 3 a.m. notification job.
        let account = ServiceAccount::parse(&test_key::service_account_json_with_key("not a pem"))
            .expect("the JSON itself is fine");

        let error = AccessTokenProvider::new(Client::new(), account).expect_err("must reject");
        assert!(
            error.to_string().contains("private_key"),
            "the message must name the field: {error}"
        );
    }

    #[test]
    fn a_valid_private_key_is_accepted_and_never_printed() {
        let provider = provider();
        let rendered = format!("{provider:?}");

        assert!(
            !rendered.contains("BEGIN PRIVATE KEY"),
            "the private key leaked into Debug output: {rendered}"
        );
        assert!(rendered.contains("[redacted]"), "{rendered}");
        assert!(rendered.contains("firebase@meno-test.iam.gserviceaccount.com"));
    }

    #[test]
    fn the_scope_is_the_messaging_one_and_narrower_than_nothing() {
        // A token granted without `firebase.messaging` is rejected by FCM with
        // `403 PERMISSION_DENIED` — indistinguishable from bad credentials, and the
        // single most likely place for this adapter to break.
        assert_eq!(
            MESSAGING_SCOPE,
            "https://www.googleapis.com/auth/firebase.messaging"
        );
    }

    #[test]
    fn the_safety_margin_shortens_the_cached_lifetime() {
        // The skew is the reason a token is never spent in its final second, where
        // Google's clock and ours may disagree.
        assert_eq!(usable_lifetime(3600), Duration::from_secs(3540));
        assert_eq!(usable_lifetime(60), Duration::ZERO);
        assert_eq!(
            usable_lifetime(0),
            Duration::ZERO,
            "an already-expired token must not underflow"
        );
    }

    #[test]
    fn a_token_is_unusable_the_moment_its_cached_lifetime_ends() {
        let fetched_at = Instant::now();
        let token = CachedToken {
            access_token: "ya29.token".to_owned(),
            expires_at: fetched_at + usable_lifetime(3600),
        };

        assert!(token.usable_at(fetched_at));
        assert!(
            !token.usable_at(fetched_at + Duration::from_secs(3540)),
            "a token must not be used at the instant it is due to expire"
        );
        assert!(!token.usable_at(fetched_at + Duration::from_secs(3600)));
    }

    #[test]
    fn a_cached_token_is_reused_and_an_expired_one_is_not() {
        // The cache decision in isolation: what `bearer` does with the answer. Reaching
        // the network here would need a mock server for a two-line comparison.
        let now = Instant::now();
        let fresh = CachedToken {
            access_token: "ya29.fresh".to_owned(),
            expires_at: now + usable_lifetime(3600),
        };
        let stale = CachedToken {
            access_token: "ya29.stale".to_owned(),
            expires_at: now + Duration::from_secs(1),
        };

        assert!(fresh.usable_at(now));
        assert!(!stale.usable_at(now + Duration::from_secs(2)));
    }

    #[test]
    fn the_assertion_carries_exactly_the_claims_google_checks() {
        // The claims are the contract with Google. `aud` and `scope` in particular are
        // the two that are easy to get subtly wrong and produce a `400` naming neither.
        let provider = provider();
        let issued_at = 1_700_000_000;

        let assertion = provider.assertion(issued_at).expect("signs");
        let claims: serde_json::Value =
            serde_json::from_slice(&decode_segment(payload_of(&assertion)))
                .expect("claims are JSON");

        assert_eq!(claims["iss"], "firebase@meno-test.iam.gserviceaccount.com");
        assert_eq!(claims["aud"], provider.account().token_uri());
        assert_eq!(claims["scope"], MESSAGING_SCOPE);
        assert_eq!(claims["iat"], issued_at);
        assert_eq!(
            claims["exp"].as_i64().unwrap_or_default() - issued_at,
            ASSERTION_LIFETIME_SECS,
            "Google caps this grant at one hour"
        );
    }

    #[test]
    fn the_assertion_is_a_three_part_rs256_jwt() {
        // The header is asserted rather than the signature: verifying it would mean
        // checking our own signing key, whereas the header is what tells Google which
        // algorithm to use, and getting that wrong fails every send.
        let assertion = provider().assertion(1_700_000_000).expect("signs");
        let header: serde_json::Value =
            serde_json::from_slice(&decode_segment(header_of(&assertion)))
                .expect("the header is JSON");

        assert_eq!(header["alg"], "RS256");
        assert_eq!(header["typ"], "JWT");
    }

    #[test]
    fn two_assertions_from_the_same_second_differ_only_by_signature() {
        // Not a test of correctness so much as a guard on the claim set being
        // rebuilt: RS256 is deterministic, so identical claims must produce identical
        // tokens. If this ever fails, a timestamp with sub-second resolution or a
        // random `jti` has crept in — which is fine, but then it must be deliberate.
        let provider = provider();

        assert_eq!(
            provider.assertion(1_700_000_000).expect("signs"),
            provider.assertion(1_700_000_000).expect("signs"),
        );
        assert_ne!(
            provider.assertion(1_700_000_000).expect("signs"),
            provider.assertion(1_700_000_001).expect("signs"),
            "the issued-at must reach the token"
        );
    }

    /// The header segment of a JWT.
    fn header_of(assertion: &str) -> &str {
        assertion.split('.').next().unwrap_or_default()
    }

    /// The claims segment of a JWT.
    fn payload_of(assertion: &str) -> &str {
        assertion.split('.').nth(1).unwrap_or_default()
    }

    /// Base64url-decode one JWT segment. A test-only helper so this module needs no
    /// extra dependency just to read a claim.
    fn decode_segment(segment: &str) -> Vec<u8> {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = Vec::new();
        let mut buffer = 0u32;
        let mut bits = 0u32;

        for byte in segment.bytes() {
            if byte == b'=' {
                break;
            }
            let value = ALPHABET
                .iter()
                .position(|c| *c == byte)
                .unwrap_or_else(|| panic!("{byte} is not base64url"))
                as u32;
            buffer = (buffer << 6) | value;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push(((buffer >> bits) & 0xFF) as u8);
            }
        }
        out
    }
}
