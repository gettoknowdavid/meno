//! Google sign-in: PKCE authorization-code flow with an enforced `email_verified`.
//!
//! Ported from `apps/api/src/infrastructure/oauth/google.rs` on `master` (`903c3ba`).
//!
//! # What changed, and why
//!
//! - **`.expect()` is gone** (§9.1). `master` had five: three on URL construction in
//!   `new` (`"Invalid Google auth URI"` and friends) and one on the HTTP client. A typo in
//!   `GOOGLE_REDIRECT_URI` is a configuration mistake, and the plan's own §7.11 names
//!   startup panics as a defect to fix.
//! - **§7.12's takeover is closed at the adapter.** `master`'s `exchange_code` returned
//!   whatever userinfo said, including `email_verified: false`; only `verify_id_token`
//!   checked it. [`IdentityProvider::exchange_code`] now refuses, so there is no path that
//!   yields an unverified identity.
//! - **Endpoints are configurable** ([`GoogleSettings`]), so the whole flow is testable
//!   against a local server. `master` hard-coded `www.googleapis.com` and
//!   `oauth2.googleapis.com`, which meant no test could run without a real Google account.
//! - **State is verified.** `master` generated a CSRF token and never compared it on the
//!   callback, so the CSRF binding existed in name only.
//! - **A `userinfo_uri`, not `tokeninfo`, for the code flow.** `master` called the
//!   OIDC userinfo endpoint for one path and the tokeninfo endpoint for the other; both
//!   return the same claims, and the code flow's bearer token is what the OIDC endpoint is
//!   for.
//!
//! # Testing
//!
//! The endpoints come from [`GoogleSettings`], so the whole request path — real PKCE
//! challenge, real form-encoded token POST, real bearer call to userinfo — runs against
//! `wiremock` with no Google account. The `#[ignore]`d tests in `mod tests::live` cover
//! what only a real consent screen can.

use std::sync::Arc;

use async_trait::async_trait;
use oauth2::basic::{
    BasicClient, BasicErrorResponse, BasicRevocationErrorResponse, BasicTokenIntrospectionResponse,
    BasicTokenResponse,
};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, PkceCodeChallenge, RedirectUrl,
    StandardRevocableToken, TokenResponse, TokenUrl,
};
use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

use crate::config::GoogleSettings;
use crate::infrastructure::oauth::error::OAuthError;
use crate::infrastructure::oauth::store::{GoogleIdentity, IdentityProvider, OAuthState, SCOPES};

/// Google's client, with every endpoint resolved.
type GoogleClient = oauth2::Client<
    BasicErrorResponse,
    BasicTokenResponse,
    BasicTokenIntrospectionResponse,
    StandardRevocableToken,
    BasicRevocationErrorResponse,
    oauth2::EndpointSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointSet,
>;

/// Google's userinfo response.
///
/// Only the fields Meno uses. `sub` and `email` are required; `email_verified` defaults to
/// **false** rather than erroring when absent, because a provider that omits the flag has
/// not verified the address — the safe reading, and the one §7.12 requires.
#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    sub: String,
    email: String,
    name: Option<String>,
    picture: Option<String>,
    #[serde(default)]
    email_verified: bool,
}

/// The production [`IdentityProvider`].
pub struct GoogleIdentityProvider {
    client: GoogleClient,
    userinfo_uri: String,
    tokeninfo_uri: String,
    http: Client,
    /// The code exchange's HTTP client, built once.
    ///
    /// Deliberately *not* the shared [`crate::infrastructure::http`] handle: `oauth2`
    /// pins reqwest 0.12 and the workspace uses 0.13, so the two `reqwest::Client`
    /// types are unrelated and cannot be passed across. What must not happen — and did
    /// — is `Client::new()` per sign-in, a fresh pool and a fresh TLS handshake for
    /// every code exchange. This field is that client, built once with the shared
    /// layer's timeouts, so the exchange reuses one pool for the process's lifetime.
    oauth_http: oauth2::reqwest::Client,
}

impl GoogleIdentityProvider {
    /// Build the provider from validated settings.
    ///
    /// Takes [`GoogleSettings`], not `&Config`: the adapter depends on seven values, and
    /// `config` is an application concern — the same correction `master`'s storage
    /// adapter needed.
    ///
    /// # Errors
    ///
    /// Returns [`OAuthError::Config`] when an endpoint or the redirect URI cannot be
    /// parsed, or when the HTTP client cannot be built. Every one of those is `master`'s
    /// `.expect`, and every one of them is reachable by a typo in a `.env`.
    pub fn new(settings: &GoogleSettings) -> Result<Arc<Self>, OAuthError> {
        let auth_uri = AuthUrl::new(settings.auth_uri.clone())
            .map_err(|e| OAuthError::Config(format!("GOOGLE_AUTH_URI is unusable: {e}")))?;
        let token_uri = TokenUrl::new(settings.token_uri.clone())
            .map_err(|e| OAuthError::Config(format!("GOOGLE_TOKEN_URI is unusable: {e}")))?;
        let redirect_uri = RedirectUrl::new(settings.redirect_uri.clone())
            .map_err(|e| OAuthError::Config(format!("GOOGLE_REDIRECT_URI is unusable: {e}")))?;

        let client = BasicClient::new(ClientId::new(settings.client_id.clone()))
            .set_client_secret(ClientSecret::new(
                settings.client_secret.expose().to_owned(),
            ))
            .set_auth_uri(auth_uri)
            .set_token_uri(token_uri)
            .set_redirect_uri(redirect_uri);

        // The shared outbound client (§5.6): one pool for token, userinfo and every
        // other adapter's calls, with the request deadline spelled in one place.
        let http = crate::infrastructure::http::shared()
            .map_err(|e| OAuthError::Config(e.to_string()))?
            .clone();

        // Built once, per the field's docs: `oauth2`'s client is a different reqwest
        // type than ours, but it obeys the same outbound policy.
        let oauth_http = oauth2::reqwest::Client::builder()
            .timeout(Duration::from_secs(
                crate::infrastructure::http::REQUEST_TIMEOUT_SECS,
            ))
            .connect_timeout(Duration::from_secs(
                crate::infrastructure::http::CONNECT_TIMEOUT_SECS,
            ))
            // See `infrastructure::http`: a 3xx from the token endpoint is a fault to
            // report, not a route to follow — and `oauth2`'s docs recommend exactly
            // this for `request_async`.
            .redirect(oauth2::reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| OAuthError::Config(format!("the HTTP client is unusable: {e}")))?;

        Ok(Arc::new(Self {
            client,
            userinfo_uri: settings.userinfo_uri.clone(),
            tokeninfo_uri: settings.tokeninfo_uri.clone(),
            http,
            oauth_http,
        }))
    }

    /// Inspect an ID token that came back on a mobile deep link.
    ///
    /// Kept separate from [`IdentityProvider::exchange_code`] because mobile apps receive
    /// an ID token directly rather than redeeming a code, so the two flows genuinely
    /// differ. Both end at the same guard, which is what closes §7.12 for both.
    ///
    /// # Errors
    ///
    /// [`OAuthError::EmailNotVerified`] for an unverified address, and
    /// [`OAuthError::Upstream`] for a provider fault.
    pub async fn identity_from_id_token(
        &self,
        id_token: &str,
    ) -> Result<GoogleIdentity, OAuthError> {
        let info = self
            .http
            .get(&self.tokeninfo_uri)
            .query(&[("id_token", id_token)])
            .send()
            .await?
            .error_for_status()
            .map_err(|e| OAuthError::Upstream(format!("tokeninfo rejected the token: {e}")))?
            .json::<GoogleUserInfo>()
            .await?;

        into_identity(info)
    }
}

/// Apply the §7.12 guard and lift the wire type into ours.
///
/// One function, used by both entry points, so the check cannot be applied to one path
/// and forgotten on the other — which is precisely how `master` shipped the bug.
fn into_identity(info: GoogleUserInfo) -> Result<GoogleIdentity, OAuthError> {
    let identity = GoogleIdentity {
        subject: info.sub,
        email: info.email,
        name: info.name,
        picture: info.picture,
        email_verified: info.email_verified,
    };

    identity.require_linkable()?;
    Ok(identity)
}

#[async_trait]
impl IdentityProvider for GoogleIdentityProvider {
    fn name(&self) -> &str {
        "Google"
    }

    async fn authorize_url(&self) -> Result<(String, OAuthState), OAuthError> {
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();

        let (url, csrf) = self
            .client
            .authorize_url(oauth2::CsrfToken::new_random)
            .add_scopes(
                SCOPES
                    .iter()
                    .map(|scope| oauth2::Scope::new((*scope).to_owned())),
            )
            .set_pkce_challenge(challenge)
            .url();

        Ok((
            url.to_string(),
            OAuthState {
                state: csrf.secret().clone(),
                verifier: verifier.secret().clone(),
            },
        ))
    }

    async fn exchange_code(
        &self,
        code: &str,
        state: &OAuthState,
    ) -> Result<GoogleIdentity, OAuthError> {
        // No CSRF check here, deliberately: by the time the handler reaches this method
        // it has already *consumed* the stored state via [`OAuthStateStore`], and an
        // unconsumable state is one that never existed. Re-comparing the stored value
        // against itself here would assert nothing while looking like it did — the exact
        // false confidence §7.12 is about.
        let token_response = self
            .client
            .exchange_code(AuthorizationCode::new(code.to_owned()))
            .set_pkce_verifier(oauth2::PkceCodeVerifier::new(state.verifier.clone()))
            .request_async(&self.oauth_http)
            .await
            .map_err(|e| OAuthError::CodeRejected(e.to_string()))?;

        let info = self
            .http
            .get(&self.userinfo_uri)
            .bearer_auth(token_response.access_token().secret())
            .send()
            .await?
            .error_for_status()
            .map_err(|e| OAuthError::Upstream(format!("userinfo rejected the token: {e}")))?
            .json::<GoogleUserInfo>()
            .await?;

        into_identity(info)
    }

    async fn verify_id_token(&self, id_token: &str) -> Result<GoogleIdentity, OAuthError> {
        // The inherent method applies `into_identity`, and therefore the §7.12 guard.
        // Duplicating the request here would give the mobile path its own copy of the
        // check, which is the failure mode this seam exists to prevent.
        self.identity_from_id_token(id_token).await
    }

    fn is_enabled(&self) -> bool {
        true
    }
}

impl std::fmt::Debug for GoogleIdentityProvider {
    /// Hand-written: a derived `Debug` would print the client secret carried by
    /// `BasicClient` (§9.5).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoogleIdentityProvider")
            .field("userinfo_uri", &self.userinfo_uri)
            .field("tokeninfo_uri", &self.tokeninfo_uri)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    //! Tests for [`GoogleIdentityProvider`].
    //!
    //! Two layers:
    //!
    //! - **Construction and the authorization URL.** Pure, and the URL is asserted field
    //!   by field — PKCE present, state present, the three scopes present — because
    //!   `master` had a CSRF token it never checked, and that is exactly the kind of thing
    //!   that regresses silently.
    //! - **The request path**, against a `wiremock` server standing in for Google. Real
    //!   form-encoded token POST, real PKCE verifier, real bearer call to userinfo. This
    //!   layer is what `master` could not have: with hard-coded endpoints there was no way
    //!   to ask what the adapter actually sends without a real Google account.

    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::config::Secret;

    /// The harness: a local server plus a provider pointed at it.
    struct Fake {
        server: MockServer,
        provider: Arc<GoogleIdentityProvider>,
    }

    impl Fake {
        async fn start() -> Self {
            let server = MockServer::start().await;
            let provider = provider_for(&server).expect("a provider");

            Self { server, provider }
        }

        /// Answer the token exchange.
        async fn with_token(&self, status: u16, body: serde_json::Value) -> &Self {
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
                .mount(&self.server)
                .await;
            self
        }

        /// Answer userinfo with a verified identity.
        async fn with_verified_user(&self, email: &str) -> &Self {
            self.with_user(json!({
                "sub": "1000",
                "email": email,
                "name": "A User",
                "picture": "https://cdn.example.com/a.png",
                "email_verified": true
            }))
            .await
        }

        /// Answer userinfo with an arbitrary body.
        async fn with_user(&self, body: serde_json::Value) -> &Self {
            Mock::given(method("GET"))
                .and(path("/userinfo"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&self.server)
                .await;
            self
        }

        async fn requests(&self) -> Vec<String> {
            self.server
                .received_requests()
                .await
                .expect("recorded requests")
                .iter()
                .map(|r| r.url.to_string())
                .collect()
        }
    }

    /// Settings pointing every endpoint at a local server.
    fn provider_for(server: &MockServer) -> Result<Arc<GoogleIdentityProvider>, OAuthError> {
        let base = server.uri();
        GoogleIdentityProvider::new(&GoogleSettings {
            client_id: "test-client".to_owned(),
            client_secret: Secret::new("test-secret"),
            redirect_uri: "https://app.example.com/auth/google/callback".to_owned(),
            auth_uri: format!("{base}/authorize"),
            token_uri: format!("{base}/token"),
            userinfo_uri: format!("{base}/userinfo"),
            tokeninfo_uri: format!("{base}/tokeninfo"),
        })
    }

    /// Settings pointing at Google's real endpoints, for the pure tests.
    fn settings() -> GoogleSettings {
        GoogleSettings {
            client_id: "test-client".to_owned(),
            client_secret: Secret::new("test-secret"),
            redirect_uri: "https://app.example.com/auth/google/callback".to_owned(),
            auth_uri: GoogleSettings::DEFAULT_AUTH_URI.to_owned(),
            token_uri: GoogleSettings::DEFAULT_TOKEN_URI.to_owned(),
            userinfo_uri: GoogleSettings::DEFAULT_USERINFO_URI.to_owned(),
            tokeninfo_uri: GoogleSettings::DEFAULT_TOKENINFO_URI.to_owned(),
        }
    }

    fn state() -> OAuthState {
        OAuthState {
            state: "abc".to_owned(),
            verifier: "a-verifier".to_owned(),
        }
    }

    // ── construction ───────────────────────────────────────────────────────

    #[test]
    fn a_configured_provider_builds_without_touching_the_network() {
        assert!(
            GoogleIdentityProvider::new(&settings()).is_ok(),
            "construction must be pure"
        );
    }

    #[test]
    fn a_malformed_redirect_uri_is_a_config_error_not_a_panic() {
        // The §9.1 regression test for `master`'s
        // `.expect("Invalid Google redirect URI")`.
        let mut broken = settings();
        broken.redirect_uri = "not a url at all".to_owned();

        let error = GoogleIdentityProvider::new(&broken).expect_err("must be rejected");

        assert!(
            matches!(error, OAuthError::Config(_)),
            "expected a config error, got {error:?}"
        );
        assert!(
            error.to_string().contains("GOOGLE_REDIRECT_URI"),
            "the message must name the variable: {error}"
        );
    }

    #[test]
    fn a_malformed_auth_uri_is_a_config_error_naming_that_variable() {
        let mut broken = settings();
        broken.auth_uri = "::::".to_owned();

        let error = GoogleIdentityProvider::new(&broken).expect_err("must be rejected");

        assert!(error.to_string().contains("GOOGLE_AUTH_URI"), "{error}");
    }

    #[test]
    fn the_client_secret_never_appears_in_debug_output() {
        let provider = GoogleIdentityProvider::new(&settings()).expect("a provider");

        let rendered = format!("{provider:?}");
        assert!(
            !rendered.contains("test-secret"),
            "the client secret must not appear in Debug output: {rendered}"
        );
    }

    // ── the authorization URL ──────────────────────────────────────────────

    #[tokio::test]
    async fn the_authorization_url_carries_pkce_state_and_the_three_scopes() {
        // `master` generated a CSRF token and never checked it. These assertions are the
        // guard against that regressing back into "a token we hand out and ignore".
        let provider = GoogleIdentityProvider::new(&settings()).expect("a provider");

        let (url, state) = provider.authorize_url().await.expect("a URL");

        assert!(url.starts_with(GoogleSettings::DEFAULT_AUTH_URI), "{url}");
        assert!(url.contains("code_challenge="), "PKCE must be sent: {url}");
        assert!(
            url.contains("code_challenge_method=S256"),
            "PKCE must use S256, never `plain`: {url}"
        );
        assert!(url.contains(&format!("state={}", state.state)), "{url}");
        // `scope` is one space-joined parameter, `+`-encoded on the wire.
        assert!(
            url.contains("scope=openid+email+profile"),
            "all three scopes must be requested: {url}"
        );
        assert!(!state.verifier.is_empty(), "the verifier must be returned");
    }

    #[tokio::test]
    async fn each_authorization_request_gets_a_fresh_challenge_and_state() {
        let provider = GoogleIdentityProvider::new(&settings()).expect("a provider");

        let (_, first) = provider.authorize_url().await.expect("a URL");
        let (_, second) = provider.authorize_url().await.expect("a URL");

        assert_ne!(
            first.state, second.state,
            "a repeated state would let one authorization serve two people"
        );
        assert_ne!(first.verifier, second.verifier);
    }

    // ── the exchange, against a local server ───────────────────────────────

    #[tokio::test]
    async fn a_verified_user_completes_the_flow() {
        let fake = Fake::start().await;
        fake.with_token(
            200,
            json!({ "access_token": "ya29.test", "token_type": "Bearer" }),
        )
        .await;
        fake.with_verified_user("user@example.com").await;

        let identity = fake
            .provider
            .exchange_code("the-code", &state())
            .await
            .expect("a verified identity");

        assert_eq!(identity.subject, "1000");
        assert_eq!(identity.email, "user@example.com");
        assert!(identity.email_verified);
        assert_eq!(identity.subject_key(), "google:1000");
    }

    #[tokio::test]
    async fn an_unverified_email_is_refused_and_userinfo_was_still_asked() {
        // §7.12. The provider was asked — the address simply is not good enough, which is
        // what makes this a refusal rather than a communication failure.
        let fake = Fake::start().await;
        fake.with_token(
            200,
            json!({ "access_token": "ya29.test", "token_type": "Bearer" }),
        )
        .await;
        fake.with_user(json!({
            "sub": "1000",
            "email": "victim@example.com",
            "email_verified": false
        }))
        .await;

        let error = fake
            .provider
            .exchange_code("the-code", &state())
            .await
            .expect_err("an unverified email must be refused");

        assert!(
            matches!(error, OAuthError::EmailNotVerified),
            "got {error:?}"
        );
        assert!(
            fake.requests()
                .await
                .iter()
                .any(|url| url.contains("/userinfo")),
            "userinfo should have been consulted: {:?}",
            fake.requests().await
        );
    }

    #[tokio::test]
    async fn a_missing_email_verified_field_is_treated_as_unverified() {
        // The fail-closed reading. `#[serde(default)]` on a bool is `false`, so a provider
        // that omits the flag does not get the benefit of the doubt — the entire point of
        // §7.12.
        let fake = Fake::start().await;
        fake.with_token(
            200,
            json!({ "access_token": "ya29.test", "token_type": "Bearer" }),
        )
        .await;
        fake.with_user(json!({ "sub": "1000", "email": "victim@example.com" }))
            .await;

        let error = fake
            .provider
            .exchange_code("the-code", &state())
            .await
            .expect_err("an absent email_verified must not be trusted");

        assert!(
            matches!(error, OAuthError::EmailNotVerified),
            "got {error:?}"
        );
    }

    #[tokio::test]
    async fn a_rejected_code_is_a_client_error_not_an_outage() {
        let fake = Fake::start().await;
        fake.with_token(
            400,
            json!({ "error": "invalid_grant", "error_description": "Bad code" }),
        )
        .await;

        let error = fake
            .provider
            .exchange_code("expired", &state())
            .await
            .expect_err("a 400 from the token endpoint must fail");

        assert!(
            matches!(error, OAuthError::CodeRejected(_)),
            "got {error:?}"
        );
        assert!(!error.is_retryable(), "a spent code cannot be retried");
    }

    #[tokio::test]
    async fn the_pkce_verifier_is_sent_to_the_token_endpoint() {
        // If the verifier is not sent, PKCE is decorative and an intercepted code is
        // replayable.
        //
        // Asserted on the *recorded request* rather than by constraining the mock's
        // matcher. A matcher-based assertion is a false negative waiting to happen: if
        // the body is encoded differently than expected, no mock matches, the server
        // answers 404, and the test reports "the exchange failed" instead of "the verifier
        // was missing". Reading the body back proves the verifier was sent *and* keeps the
        // failure message pointing at the right thing.
        let fake = Fake::start().await;
        fake.with_token(
            200,
            json!({ "access_token": "ya29.test", "token_type": "Bearer" }),
        )
        .await;
        fake.with_verified_user("user@example.com").await;

        fake.provider
            .exchange_code("the-code", &state())
            .await
            .expect("the exchange must succeed");

        let recorded = fake.server.received_requests().await.expect("requests");
        let token_request = recorded
            .iter()
            .find(|r| r.url.path() == "/token")
            .expect("a token request");
        let body = String::from_utf8_lossy(token_request.body.as_ref()).to_string();

        assert!(
            body.contains("code_verifier=a-verifier"),
            "the PKCE verifier must reach the token endpoint: {body}"
        );
        assert!(body.contains("code=the-code"), "{body}");
        assert!(
            body.contains("grant_type=authorization_code"),
            "the code verifier is only meaningful on an authorization_code grant: {body}"
        );
    }

    #[tokio::test]
    async fn the_bearer_access_token_is_what_reaches_userinfo() {
        let fake = Fake::start().await;
        fake.with_token(
            200,
            json!({ "access_token": "ya29.test", "token_type": "Bearer" }),
        )
        .await;
        fake.with_verified_user("user@example.com").await;

        fake.provider
            .exchange_code("the-code", &state())
            .await
            .expect("an exchange");

        let recorded = fake.server.received_requests().await.expect("requests");
        let userinfo = recorded
            .iter()
            .find(|r| r.url.path() == "/userinfo")
            .expect("a userinfo request");

        let auth = userinfo
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();

        assert_eq!(auth, "Bearer ya29.test");
    }

    #[tokio::test]
    async fn a_failed_token_exchange_never_reaches_userinfo() {
        let fake = Fake::start().await;
        fake.with_token(503, json!({ "error": "backend_error" }))
            .await;

        fake.provider
            .exchange_code("the-code", &state())
            .await
            .expect_err("a 503 must fail");

        assert!(
            fake.requests()
                .await
                .iter()
                .all(|url| !url.contains("/userinfo")),
            "a failed token exchange must not continue to userinfo: {:?}",
            fake.requests().await
        );
    }

    #[tokio::test]
    async fn an_id_token_inspection_applies_the_same_verified_email_guard() {
        // The mobile path. `master` checked the flag here and not in `exchange_code`;
        // both now go through `into_identity`, which is the point.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tokeninfo"))
            .and(query_param("id_token", "an-id-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "sub": "1000", "email": "victim@example.com", "email_verified": false
            })))
            .mount(&server)
            .await;

        let mut settings = settings();
        settings.tokeninfo_uri = format!("{}/tokeninfo", server.uri());
        let provider = GoogleIdentityProvider::new(&settings).expect("a provider");

        let error = provider
            .identity_from_id_token("an-id-token")
            .await
            .expect_err("an unverified email must be refused");

        assert!(
            matches!(error, OAuthError::EmailNotVerified),
            "got {error:?}"
        );
    }

    #[tokio::test]
    async fn a_verified_id_token_yields_a_linkable_identity() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tokeninfo"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "sub": "1000",
                "email": "user@example.com",
                "name": "A User",
                "picture": "https://cdn.example.com/a.png",
                "email_verified": true
            })))
            .mount(&server)
            .await;

        let mut settings = settings();
        settings.tokeninfo_uri = format!("{}/tokeninfo", server.uri());
        let provider = GoogleIdentityProvider::new(&settings).expect("a provider");

        let identity = provider
            .identity_from_id_token("an-id-token")
            .await
            .expect("a verified identity");

        assert_eq!(identity.name.as_deref(), Some("A User"));
        assert_eq!(
            identity.picture.as_deref(),
            Some("https://cdn.example.com/a.png")
        );
        identity.require_linkable().expect("must be linkable");
    }

    /// Tests that need a real Google client and a real consent screen.
    ///
    /// Nothing here runs without real `GOOGLE_*` credentials, so everything is
    /// `#[ignore]`d. Re-enable with
    /// `cargo test -p meno-api -- --ignored oauth::google::tests::live`.
    mod live {

        use super::*;

        #[tokio::test]
        #[ignore = "needs real GOOGLE_* credentials and a browser"]
        async fn the_live_consent_screen_round_trips() {
            let config = crate::config::Config::load().expect("a real environment");
            let settings = config.google.as_ref().expect("Google must be enabled");
            let provider = GoogleIdentityProvider::new(settings).expect("a provider");

            // §9.2 denies `print_stderr`; the URL has to reach the operator somehow, and
            // `tracing` is the channel the plan says every event goes through.
            let (url, state) = provider.authorize_url().await.expect("a URL");
            tracing::warn!(
                authorize_url = %url,
                state = %state.state,
                "open the URL in a browser, then set OAUTH_TEST_CODE to the code it returns"
            );

            // A human completes the consent screen here. The exchange is the part worth
            // asserting, and it is the part `master` had no way to test.
            let code = std::env::var("OAUTH_TEST_CODE").expect("OAUTH_TEST_CODE");
            let identity = provider
                .exchange_code(&code, &state)
                .await
                .expect("a verified identity");

            assert!(identity.email_verified);
        }
    }
}
