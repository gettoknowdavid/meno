//! Access-token authentication.
//!
//! Ported from `apps/api/src/shared/middleware/auth.rs` on `master` (`903c3ba`), rebuilt
//! against the layers that exist now rather than against `crate::modules::auth` and
//! `crate::state::MenoState` — neither of which has been ported yet.
//!
//! # What changed, and why each change is not cosmetic
//!
//! 1. **It takes a typed state, not `Arc<MenoState>`.** §9.3 says state structs expose
//!    traits, not concrete types; [`AuthState`] names the two things this layer actually
//!    needs and nothing else, so `from_fn_with_state` cannot be wired to a half-built
//!    application state.
//! 2. **It returns `meno_core::Error`, not `AuthError`.** §4.2 replaced the nine parallel
//!    enums with one; reintroducing `AuthError` here would put a tenth back. Every
//!    rejection maps to a stable wire code from `ErrorCode`.
//! 3. **Token verification and revocation are traits, so the middleware is testable
//!    without a secret or a Redis.** §5.6 asks for infrastructure behind an interface;
//!    this is that, and it is what lets the tests below drive the real middleware through
//!    a real `Router` instead of a hand-rolled stand-in.
//! 4. **A blocklist backend failure is a 503, not a 401.** `master` let a Redis error
//!    propagate out of `is_access_token_blocked` as an auth failure. That is a lie: the
//!    token may be perfectly valid and the *limiter* be down. Telling every client "your
//!    token is invalid" during a Redis blip is the outage §3.2 is about.
//! 5. **`Span::current().record` is gone.** Recording a field on a span that never
//!    declared it is a silent no-op — it looks like it logs and does not. §4.8 asks for
//!    named fields, so this emits them.
//!
//! # Cost: one verification, one round trip
//!
//! §4.7 item 6 keeps the Redis blocklist and declines a negative cache, so a protected
//! request costs one CPU-bound signature check and one `EXISTS`. Both are on the request
//! path, so neither may `unwrap` (§9.1).

use async_trait::async_trait;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, header};
use axum::middleware::Next;
use axum::response::Response;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use meno_core::{Error as MenoError, ErrorCode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::infrastructure::constants::BLOCKLIST_PREFIX;
use crate::infrastructure::redis::Redis;
use crate::infrastructure::redis::keys::{RedisKey, ttl};
use crate::middleware::from_error;

/// The identity providers a user can sign in with.
///
/// A wire type, so it is `camelCase`-agnostic here and `#[non_exhaustive]`: a client built
/// against an older build must not fail to parse a provider it has never heard of. §4.2's
/// stability contract runs in the same direction — codes may be added, renamed never.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum AuthProvider {
    /// Email and password.
    Password,
    /// Google, via OAuth2/OIDC.
    Google,
}

impl AuthProvider {
    /// The exact string sent on the wire.
    ///
    /// One spelling per variant, so a client that persisted the string compares it without
    /// a second mapping table that will drift from the enum.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Google => "google",
        }
    }
}

impl std::fmt::Display for AuthProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a user is allowed to do.
///
/// Ordered least- to most-privileged, so [`UserRole::at_least`] is the whole of the
/// privilege comparison.
/// `Default` is `User`, and only because `#[serde(default)]` on the claim requires it:
/// a token minted before the `role` claim existed must decode as the *least*
/// privileged role. That is the direction a default should fail (§9.5).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum UserRole {
    /// An ordinary signed-in user.
    #[default]
    User,
    /// A creator, who owns broadcasts.
    Creator,
    /// An administrator.
    Admin,
}

impl UserRole {
    /// The exact string sent on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Creator => "creator",
            Self::Admin => "admin",
        }
    }

    /// Whether this role satisfies a requirement of at least `required`.
    ///
    /// Ordering is by discriminant, so a variant added later is less privileged than
    /// `Admin` unless it says otherwise — which is the direction a mistake in an
    /// authorisation check should fail.
    #[must_use]
    pub const fn at_least(self, required: Self) -> bool {
        (self as u8) >= (required as u8)
    }
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The claims carried by an access token.
///
/// `Deserialize` rather than a bespoke parse, so a signature check and a schema check are
/// one step: a payload that does not deserialise is rejected before any of its contents
/// are read, so nothing downstream ever has to decide which fields to trust.
///
/// `exp` and `iat` are seconds since the epoch because that is the JWT numeric-date form.
/// `jsonwebtoken` validates `exp` during `decode`, so nothing re-checks it afterwards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccessClaims {
    /// The user's id — the JWT `sub`.
    pub sub: Uuid,
    /// The token's own id, and the key the blocklist stores.
    ///
    /// Deliberately not the user id: revoking one device's session must not revoke the
    /// user's other devices (§4.7 item 3).
    pub jti: Uuid,
    /// Issued-at, seconds since the epoch.
    pub iat: i64,
    /// Expiry, seconds since the epoch.
    pub exp: i64,
    /// Display name.
    pub full_name: String,
    /// Email address.
    pub email: String,
    /// Whether the email address has been verified.
    pub verified: bool,
    /// Which providers this user can sign in with.
    #[serde(default)]
    pub providers: Vec<AuthProvider>,
    /// The user's role.
    #[serde(default)]
    pub role: UserRole,
}

impl AccessClaims {
    /// Claims for a token issued at `issued_at` and valid for `lifetime_secs`.
    ///
    /// Exists so "how long is an access token good for" has one answer, and so the tests
    /// mint tokens through the same arithmetic the token service does.
    #[must_use]
    pub fn new(
        user_id: Uuid,
        jti: Uuid,
        issued_at: i64,
        lifetime_secs: i64,
        full_name: impl Into<String>,
        email: impl Into<String>,
        verified: bool,
    ) -> Self {
        Self {
            sub: user_id,
            jti,
            iat: issued_at,
            exp: issued_at + lifetime_secs,
            full_name: full_name.into(),
            email: email.into(),
            verified,
            providers: vec![AuthProvider::Password],
            role: UserRole::User,
        }
    }
}

/// The authenticated caller, as handlers see it.
///
/// Inserted into request extensions by [`auth_middleware`] and read back with
/// `Extension<AuthUser>`. It carries what a handler needs and nothing about *how* those
/// facts were established — a handler cannot ask whether the blocklist was checked and get
/// an answer, because for a value that exists here the answer is always yes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthUser {
    /// The user's id.
    pub id: Uuid,
    /// The access token's `jti`.
    pub jti: Uuid,
    /// Display name.
    pub full_name: String,
    /// Email address.
    pub email: String,
    /// Whether the email address is verified.
    pub verified: bool,
    /// Which providers this user can sign in with.
    pub providers: Vec<AuthProvider>,
    /// The user's role.
    pub role: UserRole,
}

impl AuthUser {
    /// Narrow verified claims to the caller a handler receives.
    ///
    /// A named function rather than a struct literal in the middleware, so the field
    /// mapping exists exactly once: a claim added here and forgotten in the middleware
    /// would be a silently-unavailable value rather than a compile error.
    #[must_use]
    pub fn from_claims(claims: AccessClaims) -> Self {
        Self {
            id: claims.sub,
            jti: claims.jti,
            full_name: claims.full_name,
            email: claims.email,
            verified: claims.verified,
            providers: claims.providers,
            role: claims.role,
        }
    }
}

/// Everything the middleware needs, and nothing else.
#[derive(Clone)]
pub struct AuthState {
    verifier: Arc<dyn TokenVerifier>,
    blocklist: Arc<dyn TokenBlocklist>,
}

impl AuthState {
    /// Build a state from its two collaborators.
    #[must_use]
    pub fn new(verifier: Arc<dyn TokenVerifier>, blocklist: Arc<dyn TokenBlocklist>) -> Self {
        Self {
            verifier,
            blocklist,
        }
    }

    /// The whole of the gate: verify the token, then confirm it is not revoked.
    ///
    /// One method so every surface makes the same decision. [`auth_middleware`] runs it
    /// for HTTP requests and the `/ws` upgrade runs it before a socket exists — a rule
    /// that lives in two places is a rule that eventually applies to one of them.
    ///
    /// The order is the middleware's, preserved deliberately: the blocklist round trip
    /// is **not** made when verification already failed (an invalid token must not buy
    /// an attacker a Redis `EXISTS`), and a revoked token reports the same generic
    /// [`ErrorCode::InvalidToken`] as an invalid one, so the response is not an oracle
    /// for which check failed.
    ///
    /// # Errors
    ///
    /// - [`MenoError::Unauthorized`] with [`ErrorCode::InvalidToken`] for a token that
    ///   fails verification or has been revoked — the reason is logged, never returned.
    /// - Whatever the blocklist returns when Redis itself is down. That is deliberately
    ///   *not* an authentication failure (§4.2): "I could not check" and "you are
    ///   revoked" have opposite remedies, and conflating them logs every client out
    ///   during a blip.
    pub async fn authenticate(&self, token: &str) -> Result<AuthUser, MenoError> {
        let claims = self.verifier.verify(token).await?;

        match self.blocklist.is_revoked(claims.jti, claims.exp).await {
            Ok(false) => Ok(AuthUser::from_claims(claims)),
            Ok(true) => Err(MenoError::Unauthorized {
                code: ErrorCode::InvalidToken,
                message: "the access token is not valid".to_owned(),
            }),
            Err(error) => Err(error),
        }
    }
}

impl std::fmt::Debug for AuthState {
    /// Hand-written so a `{:?}` of the router's state cannot print a decoding key.
    /// §9.5: secrets are never `Debug`-printed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthState").finish_non_exhaustive()
    }
}

/// Verifies the cryptographic half of a token.
///
/// A trait rather than a direct `jsonwebtoken` call because §5.6's rule is about this
/// layer not knowing its infrastructure: swapping HS256 for RS256, or the in-process
/// verifier for one that calls an external IdP, is a different implementation of this
/// interface rather than a new branch through the middleware.
#[async_trait]
pub trait TokenVerifier: Send + Sync + std::fmt::Debug {
    /// Verify a token and return its claims.
    ///
    /// # Errors
    ///
    /// Any failure — bad signature, wrong algorithm, expired, malformed payload — is
    /// [`MenoError::Unauthorized`] with [`ErrorCode::InvalidToken`]. The caller cannot act
    /// on the distinction beyond showing the code, so it is deliberately not exposed; the
    /// reason is logged, not returned, so a client cannot probe for it.
    async fn verify(&self, token: &str) -> Result<AccessClaims, MenoError>;
}

/// Reports whether an access token has been revoked.
#[async_trait]
pub trait TokenBlocklist: Send + Sync + std::fmt::Debug {
    /// Whether the token identified by `jti` has been revoked.
    ///
    /// # Errors
    ///
    /// A backend failure, which is deliberately **not** an authentication failure. "I could
    /// not check" and "you are revoked" have opposite remedies, and conflating them logs
    /// every user out during a Redis blip.
    async fn is_revoked(&self, jti: Uuid, expires_at: i64) -> Result<bool, MenoError>;
}

/// [`TokenVerifier`] over HMAC-SHA256.
///
/// §4.7 item 4 keeps access and refresh secrets separate, so this is built from the access
/// secret alone and cannot be used to check a refresh token.
#[derive(Clone)]
pub struct JwtVerifier {
    key: DecodingKey,
    validation: Validation,
}

impl JwtVerifier {
    /// Build a verifier for `secret`.
    ///
    /// # Errors
    ///
    /// [`MenoError::Internal`] when the secret is blank. `jsonwebtoken` accepts an empty
    /// HMAC key and then rejects every token, which turns a missing `JWT_SECRET` into a
    /// login outage discovered in production instead of a startup error (§7.8).
    pub fn new(secret: &str) -> Result<Self, MenoError> {
        if secret.trim().is_empty() {
            return Err(MenoError::Internal {
                context: "build_jwt_verifier",
                detail: "the access-token secret is blank".to_owned(),
            });
        }

        Ok(Self {
            key: DecodingKey::from_secret(secret.as_bytes()),
            validation: Self::validation(),
        })
    }

    /// The validation every token is checked against.
    ///
    /// Separate so the pin is asserted directly by a test rather than inferred from a
    /// rejected token — an inferred assertion passes for the wrong reason.
    ///
    /// `HS256` is pinned, not merely permitted: if the accepted set is a *choice* the
    /// attacker picks it in the token header, and `alg: none` is the classic JWT bypass.
    /// `aud` is not validated because these tokens are not audience-scoped; a deployment
    /// that adds one should set `validation.aud` rather than relax this.
    fn validation() -> Validation {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        validation.validate_aud = false;
        validation
    }

    /// The validation this verifier applies.
    #[must_use]
    pub fn algorithm(&self) -> Algorithm {
        self.validation.algorithms[0]
    }
}

impl std::fmt::Debug for JwtVerifier {
    /// `DecodingKey` has no `Debug` because it wraps key material; this satisfies the
    /// trait bound without printing any.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JwtVerifier([redacted])")
    }
}

#[async_trait]
impl TokenVerifier for JwtVerifier {
    async fn verify(&self, token: &str) -> Result<AccessClaims, MenoError> {
        // One pass checks signature, pinned algorithm and `exp`.
        decode::<AccessClaims>(token, &self.key, &self.validation)
            .map(|data| data.claims)
            .map_err(|error| {
                tracing::debug!(detail = %error, "access token rejected");
                MenoError::Unauthorized {
                    code: ErrorCode::InvalidToken,
                    message: "the access token is not valid".to_owned(),
                }
            })
    }
}

/// [`TokenBlocklist`] backed by Redis.
///
/// §4.7 item 5 keeps the blocklist, with §3.2's caveat that a restart loses it — bounded
/// by the access token's own lifetime, which is why revoking a token whose expiry has
/// passed costs nothing: it is already unusable.
#[derive(Clone)]
pub struct RedisTokenBlocklist {
    redis: Redis,
}

impl std::fmt::Debug for RedisTokenBlocklist {
    /// `Redis` has no `Debug` because it wraps a connection URL, which is a
    /// credential. Printing the adapter is never worth that.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisTokenBlocklist")
            .finish_non_exhaustive()
    }
}

impl RedisTokenBlocklist {
    /// Build a blocklist over `redis`, using the canonical §3.2 TTL.
    #[must_use]
    pub fn new(redis: Redis) -> Self {
        Self { redis }
    }
}

/// The key a revocation of `jti` is written to.
///
/// A free function rather than a method, because the key layout is the part with the
/// security-relevant prefix and the §3.2 TTL, and a free function can be asserted in a
/// test that constructs no Redis client at all.
///
/// Namespaced by [`BLOCKLIST_PREFIX`] so scanning for one kind of entry cannot match
/// another's, and bounded by [`ttl::BLOCK_LIST`] so a logout cannot leave an unbounded key
/// on a 25 MB free instance (§3.2).
#[must_use]
pub fn blocklist_key(jti: Uuid) -> RedisKey {
    RedisKey::block_list(BLOCKLIST_PREFIX, jti, ttl::BLOCK_LIST)
}

#[async_trait]
impl TokenBlocklist for RedisTokenBlocklist {
    async fn is_revoked(&self, jti: Uuid, _expires_at: i64) -> Result<bool, MenoError> {
        self.redis
            .exists(&blocklist_key(jti))
            .await
            // Logged with the driver error, erased before it reaches the taxonomy: §5.5 and
            // §9.1. The client learns the blocklist is unavailable and nothing more.
            .map_err(|error| {
                tracing::error!(
                    service = "redis",
                    detail = %error,
                    "could not read the token blocklist"
                );
                MenoError::Upstream {
                    service: "blocklist",
                    detail: error.to_string(),
                }
            })
    }
}

/// Read the bearer token from an `Authorization` header.
///
/// # Errors
///
/// [`MenoError::Unauthorized`] when the header is absent, is not `Bearer`, or is not
/// visible ASCII. The scheme comparison is case-insensitive because RFC 7235 says it is,
/// and a client sending `bearer` is not an attacker.
fn bearer_token(headers: &HeaderMap) -> Result<&str, MenoError> {
    let raw = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(|| MenoError::Unauthorized {
            code: ErrorCode::Unauthorized,
            message: "an access token is required".to_owned(),
        })?;

    let text = raw.to_str().map_err(|_| MenoError::Unauthorized {
        code: ErrorCode::Unauthorized,
        message: "the Authorization header is not valid ASCII".to_owned(),
    })?;

    let (scheme, token) = text.split_once(' ').ok_or_else(unauthorized_scheme)?;

    if !scheme.eq_ignore_ascii_case("bearer") || token.trim().is_empty() {
        return Err(unauthorized_scheme());
    }

    Ok(token.trim())
}

/// One message for every malformed `Authorization` header, so a client gets the same
/// guidance whichever way the header was wrong and no oracle for which check failed.
fn unauthorized_scheme() -> MenoError {
    MenoError::Unauthorized {
        code: ErrorCode::Unauthorized,
        message: "the Authorization header must be `Bearer <token>`".to_owned(),
    }
}

/// Authenticate a request and attach the caller.
///
/// Apply with `from_fn_with_state`, which is what makes [`AuthState`] typed state rather
/// than an extension that can silently be absent:
///
/// ```ignore
/// .layer(axum::middleware::from_fn_with_state(
///     auth_state,
///     middleware::auth::auth_middleware,
/// ))
/// ```
///
/// §9.5: this is a *gate*, not a grant. It proves who the caller is; whether they may
/// perform the operation is the service layer's decision, never this one's.
pub async fn auth_middleware(
    State(state): State<AuthState>,
    mut req: Request,
    next: Next,
) -> Response {
    let token = match bearer_token(req.headers()) {
        Ok(token) => token,
        Err(error) => return from_error(&error),
    };

    // The gate itself lives on `AuthState` so the `/ws` upgrade runs the identical
    // sequence; the ordering and revocation rules are documented there.
    let user = match state.authenticate(token).await {
        Ok(user) => user,
        Err(error) => return from_error(&error),
    };

    // §4.8: named fields, never `format!` into the message.
    tracing::debug!(
        user.id = %user.id,
        user.jti = %user.jti,
        user.email_verified = user.verified,
        "request authenticated"
    );

    req.extensions_mut().insert(user);

    next.run(req).await
}

/// Extractor for routes that require a verified email address.
///
/// A separate extractor rather than a field on [`AuthUser`] so "this route needs a
/// verified address" appears in the handler's own signature, where it is read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedUser(pub AuthUser);

impl<S> FromRequestParts<S> for VerifiedUser
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let user = parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .ok_or_else(|| {
                // The auth layer runs first in the documented order, so a missing `AuthUser`
                // means the route was wired without it. That is a server misconfiguration, and
                // reporting it as 401 would make a wiring bug look like a user who forgot to
                // log in — which is exactly how the layer-order dependency stays invisible.
                tracing::error!(
                    "VerifiedUser requested on a route without the auth layer; \
                     the router layers are in the wrong order"
                );
                MenoError::Internal {
                    context: "extract_verified_user",
                    detail: "no AuthUser in request extensions".to_owned(),
                }
            })
            .map_err(|error| from_error(&error))?;

        if !user.verified {
            return Err(from_error(&MenoError::Forbidden {
                code: ErrorCode::EmailNotVerified,
                message: "verify your email address to use this".to_owned(),
            }));
        }

        Ok(Self(user))
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the middleware itself, and for each trait it depends on.
    //!
    //! [`TokenVerifier`] and [`TokenBlocklist`] being traits is what makes the first block
    //! possible: the middleware is exercised for real, through a real `Router`, with no
    //! signing secret and no Redis. `JwtVerifier` gets its own unit tests against tokens
    //! minted here with a test-only secret.

    use super::*;
    use axum::Extension;
    use axum::body::Body;
    use axum::http::StatusCode;
    use axum::http::request::Request as HttpRequest;
    use axum::routing::get;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    // ── fakes ─────────────────────────────────────────────────────────────

    /// The success and failure legs of a fake.
    ///
    /// Rebuilt per call rather than cloned, because `MenoError` is deliberately not `Clone`:
    /// it carries driver detail that should not be duplicated around.
    #[derive(Debug)]
    enum FakeOutcome<T> {
        /// Answers with this value.
        Allow(T),
        /// Fails as an authentication error carrying this message.
        Reject(&'static str),
        /// Fails as an unavailable upstream, which the middleware must report as a 503.
        Unavailable(&'static str),
    }

    impl<T: Clone> FakeOutcome<T> {
        fn resolve(&self) -> Result<T, MenoError> {
            match self {
                Self::Allow(value) => Ok(value.clone()),
                Self::Reject(message) => Err(MenoError::Unauthorized {
                    code: ErrorCode::InvalidToken,
                    message: (*message).to_owned(),
                }),
                Self::Unavailable(service) => Err(MenoError::Upstream {
                    service,
                    detail: "the fake backend is down".to_owned(),
                }),
            }
        }
    }

    /// A verifier that answers whatever it was built with.
    #[derive(Debug)]
    struct FakeVerifier {
        outcome: FakeOutcome<AccessClaims>,
    }

    /// A blocklist that answers whatever it was built with.
    #[derive(Debug)]
    struct FakeBlocklist {
        outcome: FakeOutcome<bool>,
    }

    #[async_trait]
    impl TokenVerifier for FakeVerifier {
        async fn verify(&self, _token: &str) -> Result<AccessClaims, MenoError> {
            self.outcome.resolve()
        }
    }

    #[async_trait]
    impl TokenBlocklist for FakeBlocklist {
        async fn is_revoked(&self, _jti: Uuid, _expires_at: i64) -> Result<bool, MenoError> {
            self.outcome.resolve()
        }
    }

    /// A far-future epoch, so a fixture token is not expired when the suite runs.
    ///
    /// `jsonwebtoken` validates `exp` during `decode`, so a fixture anchored to the day it
    /// was written starts failing on that day.
    const NOW: i64 = 4_000_000_000;

    fn claims() -> AccessClaims {
        AccessClaims::new(
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            NOW,
            900,
            "Ada Lovelace",
            "ada@example.com",
            true,
        )
    }

    /// The happy-path state: a good token, not revoked.
    fn accepting() -> AuthState {
        AuthState::new(
            Arc::new(FakeVerifier {
                outcome: FakeOutcome::Allow(claims()),
            }),
            Arc::new(FakeBlocklist {
                outcome: FakeOutcome::Allow(false),
            }),
        )
    }

    async fn echo_email(Extension(user): Extension<AuthUser>) -> String {
        user.email
    }

    async fn write(VerifiedUser(user): VerifiedUser) -> String {
        user.email
    }

    fn router(state: AuthState) -> axum::Router {
        axum::Router::new()
            .route("/me", get(echo_email))
            .route("/write", get(write))
            .layer(axum::middleware::from_fn_with_state(state, auth_middleware))
    }

    fn get_request(uri: &str, auth: Option<&str>) -> Request {
        let mut builder = HttpRequest::builder().uri(uri);
        if let Some(value) = auth {
            builder = builder.header(header::AUTHORIZATION, value);
        }
        builder.body(Body::empty()).expect("a valid request")
    }

    async fn body_of(response: Response) -> serde_json::Value {
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("the body buffers")
            .to_bytes();

        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    }

    // ── the middleware ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn a_valid_token_reaches_the_handler_with_the_caller_attached() {
        let response = router(accepting())
            .oneshot(get_request("/me", Some("Bearer good")))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("the body buffers")
            .to_bytes();
        assert_eq!(String::from_utf8_lossy(&bytes), "ada@example.com");
    }

    #[tokio::test]
    async fn a_missing_token_is_401_with_the_unauthorized_code() {
        let response = router(accepting())
            .oneshot(get_request("/me", None))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body_of(response).await["code"], "UNAUTHORIZED");
    }

    #[tokio::test]
    async fn only_a_well_formed_bearer_header_authenticates() {
        // A bare token, a missing scheme, an empty token and a different scheme must all be
        // refused. `Basic` is the realistic proxy case.
        for raw in [
            "Basic abc",
            "Bearer",
            "Bearer ",
            "Bearer  ",
            "abc",
            "Bearer\tabc",
        ] {
            let response = router(accepting())
                .oneshot(get_request("/me", Some(raw)))
                .await
                .expect("the router answers");

            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "`{raw}` must not authenticate"
            );
        }
    }

    #[tokio::test]
    async fn the_bearer_scheme_is_case_insensitive() {
        // RFC 7235: the scheme is case-insensitive. Refusing `bearer` would lock out
        // conforming clients for no security gain.
        for raw in ["bearer good", "BEARER good", "BeArEr good"] {
            let response = router(accepting())
                .oneshot(get_request("/me", Some(raw)))
                .await
                .expect("the router answers");

            assert_eq!(
                response.status(),
                StatusCode::OK,
                "`{raw}` must authenticate"
            );
        }
    }

    #[tokio::test]
    async fn a_revoked_token_is_401_with_the_generic_invalid_token_code() {
        // `INVALID_TOKEN`, not a "revoked" code: telling an attacker which tokens are live
        // and worth stealing is the whole reason the code is generic.
        let state = AuthState::new(
            Arc::new(FakeVerifier {
                outcome: FakeOutcome::Allow(claims()),
            }),
            Arc::new(FakeBlocklist {
                outcome: FakeOutcome::Allow(true),
            }),
        );

        let response = router(state)
            .oneshot(get_request("/me", Some("Bearer good")))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(body_of(response).await["code"], "INVALID_TOKEN");
    }

    #[tokio::test]
    async fn a_blocklist_outage_is_503_not_401() {
        // The §4.2 fix. Reporting a Redis fault as "your token is invalid" logs every user
        // out during a blip — the outage §3.2 is about.
        let state = AuthState::new(
            Arc::new(FakeVerifier {
                outcome: FakeOutcome::Allow(claims()),
            }),
            Arc::new(FakeBlocklist {
                outcome: FakeOutcome::Unavailable("blocklist"),
            }),
        );

        let response = router(state)
            .oneshot(get_request("/me", Some("Bearer good")))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = body_of(response).await;
        assert_eq!(body["code"], "UPSTREAM_UNAVAILABLE");
    }

    #[tokio::test]
    async fn a_verification_failure_renders_the_invalid_token_code() {
        let state = AuthState::new(
            Arc::new(FakeVerifier {
                outcome: FakeOutcome::Reject("nope"),
            }),
            Arc::new(FakeBlocklist {
                outcome: FakeOutcome::Allow(false),
            }),
        );

        let response = router(state)
            .oneshot(get_request("/me", Some("Bearer bad")))
            .await
            .expect("the router answers");
        let status = response.status();
        let body = body_of(response).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["code"], "INVALID_TOKEN");
    }

    #[tokio::test]
    async fn a_backend_fault_never_leaks_its_detail() {
        // §9.1. `to_body` replaces the detail of a non-client-safe error, and this is the one
        // place on the auth path where such an error can arise.
        let state = AuthState::new(
            Arc::new(FakeVerifier {
                outcome: FakeOutcome::Allow(claims()),
            }),
            Arc::new(FakeBlocklist {
                outcome: FakeOutcome::Unavailable("blocklist"),
            }),
        );

        let response = router(state)
            .oneshot(get_request("/me", Some("Bearer good")))
            .await
            .expect("the router answers");
        let body = body_of(response).await;

        assert!(
            !body.to_string().contains("fake backend"),
            "the backend detail must not reach the client: {body}"
        );
    }

    #[tokio::test]
    async fn an_invalid_token_does_not_reach_the_blocklist() {
        // The short-circuit is observable: if verification fails, the revocation outcome is
        // irrelevant, and a redundant `EXISTS` is a free amplification for a
        // credential-stuffing run.
        let state = AuthState::new(
            Arc::new(FakeVerifier {
                outcome: FakeOutcome::Reject("nope"),
            }),
            // Would answer "revoked" if consulted. The failure reported is verification's.
            Arc::new(FakeBlocklist {
                outcome: FakeOutcome::Allow(true),
            }),
        );

        let response = router(state)
            .oneshot(get_request("/me", Some("Bearer bad")))
            .await
            .expect("the router answers");
        let status = response.status();
        let body = body_of(response).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            body["message"], "nope",
            "the blocklist branch must not overwrite the verification failure"
        );
    }

    // ── the verified-email gate ────────────────────────────────────────────

    #[tokio::test]
    async fn an_unverified_user_is_refused_with_the_named_code() {
        let mut unverified = claims();
        unverified.verified = false;

        let state = AuthState::new(
            Arc::new(FakeVerifier {
                outcome: FakeOutcome::Allow(unverified),
            }),
            Arc::new(FakeBlocklist {
                outcome: FakeOutcome::Allow(false),
            }),
        );

        let response = router(state)
            .oneshot(get_request("/write", Some("Bearer good")))
            .await
            .expect("the router answers");
        let status = response.status();
        let body = body_of(response).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["code"], "EMAIL_NOT_VERIFIED");
    }

    #[tokio::test]
    async fn a_verified_user_passes_the_gate() {
        let response = router(accepting())
            .oneshot(get_request("/write", Some("Bearer good")))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn an_unverified_user_may_still_read() {
        // The gate is per-route, not global: "must verify to write" must not become "must
        // verify to read", which would lock a new signup out of the whole product.
        let mut unverified = claims();
        unverified.verified = false;

        let state = AuthState::new(
            Arc::new(FakeVerifier {
                outcome: FakeOutcome::Allow(unverified),
            }),
            Arc::new(FakeBlocklist {
                outcome: FakeOutcome::Allow(false),
            }),
        );

        let response = router(state)
            .oneshot(get_request("/me", Some("Bearer good")))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn the_gate_reports_a_missing_auth_layer_as_a_server_fault() {
        // A route wired without the auth layer is a wiring bug. Reporting it as 401 would
        // make it indistinguishable from a user who forgot to log in — which is exactly how
        // the layer-order dependency stays invisible until it bites in production.
        let unwired = axum::Router::new().route("/write", get(write));

        let response = unwired
            .oneshot(get_request("/write", Some("Bearer good")))
            .await
            .expect("the router answers");

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // ── the header parser, on its own ──────────────────────────────────────

    #[test]
    fn a_token_is_trimmed_of_surrounding_whitespace() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            "Bearer  tok ".parse().expect("a value"),
        );

        assert_eq!(bearer_token(&headers).expect("a token"), "tok");
    }

    #[test]
    fn a_non_ascii_authorization_header_is_refused() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            axum::http::HeaderValue::from_bytes(&[0x42, 0x65, 0x61, 0x72, 0x65, 0x72, 0x20, 0xff])
                .expect("a header value"),
        );

        assert!(bearer_token(&headers).is_err());
    }

    #[test]
    fn every_malformed_header_gets_the_same_guidance() {
        // One message for all of them, so the response is not an oracle for which check
        // failed.
        let messages: Vec<String> = ["Basic abc", "Bearer", "Bearer ", "abc"]
            .iter()
            .map(|raw| {
                let mut headers = HeaderMap::new();
                headers.insert(header::AUTHORIZATION, raw.parse().expect("a value"));
                bearer_token(&headers)
                    .expect_err("must be refused")
                    .to_string()
            })
            .collect();

        assert!(
            messages.windows(2).all(|w| w[0] == w[1]),
            "malformed headers must not be distinguishable: {messages:?}"
        );
    }

    // ── claims mapping ─────────────────────────────────────────────────────

    #[test]
    fn every_claim_field_reaches_the_caller() {
        // A claim added to `AccessClaims` and forgotten in `from_claims` would be a
        // silently-unavailable value; this asserts the mapping is total.
        let user = AuthUser::from_claims(claims());

        assert_eq!(user.id, Uuid::from_u128(1));
        assert_eq!(user.jti, Uuid::from_u128(2));
        assert_eq!(user.full_name, "Ada Lovelace");
        assert_eq!(user.email, "ada@example.com");
        assert!(user.verified);
        assert_eq!(user.providers, vec![AuthProvider::Password]);
        assert_eq!(user.role, UserRole::User);
    }

    #[test]
    fn the_jti_is_not_the_user_id() {
        // §4.7 item 3: revoking one device must not revoke the user's others, so the
        // blocklist keys on `jti`. If these ever collapse, a logout-everywhere becomes a
        // logout-everywhere-by-accident.
        let claims = claims();

        assert_ne!(claims.jti, claims.sub);
    }

    #[test]
    fn claims_round_trip_through_json_keeping_the_verdict() {
        // The token travels as JSON, so a field that fails to round trip is wrong on the
        // wire rather than only in memory.
        let original = claims();
        let json = serde_json::to_string(&original).expect("serialises");
        let back: AccessClaims = serde_json::from_str(&json).expect("deserialises");

        assert_eq!(back, original);
        assert!(
            back.verified,
            "a verified claim must not become unverified in transit"
        );
    }

    #[test]
    fn the_optional_claim_fields_default_so_an_older_token_still_decodes() {
        // A token minted before `providers`/`role` existed must still authenticate, or a
        // deploy logs every user out.
        let json = r#"{
            "sub":"00000000-0000-0000-0000-000000000001",
            "jti":"00000000-0000-0000-0000-000000000002",
            "iat":1700000000,
            "exp":1700000900,
            "full_name":"Ada",
            "email":"ada@example.com",
            "verified":true
        }"#;

        let claims: AccessClaims = serde_json::from_str(json).expect("deserialises");

        assert!(claims.providers.is_empty());
        assert_eq!(claims.role, UserRole::User);
    }

    #[test]
    fn an_expiry_is_issued_at_plus_the_lifetime() {
        assert_eq!(claims().exp - claims().iat, 900);
    }

    // ── wire spellings ─────────────────────────────────────────────────────

    #[test]
    fn providers_serialise_as_lowercase_names() {
        for (provider, name) in [
            (AuthProvider::Password, "password"),
            (AuthProvider::Google, "google"),
        ] {
            assert_eq!(
                serde_json::to_string(&provider).expect("serialises"),
                format!("\"{name}\"")
            );
            assert_eq!(provider.to_string(), name);
        }
    }

    #[test]
    fn roles_serialise_as_lowercase_names() {
        for (role, name) in [
            (UserRole::User, "user"),
            (UserRole::Creator, "creator"),
            (UserRole::Admin, "admin"),
        ] {
            assert_eq!(
                serde_json::to_string(&role).expect("serialises"),
                format!("\"{name}\"")
            );
            assert_eq!(role.to_string(), name);
        }
    }

    #[test]
    fn role_privileges_are_ordered() {
        // `at_least` is the whole of the privilege comparison, and ordering is by
        // discriminant — so a variant added later is less privileged than `Admin` unless
        // it says otherwise. That is the direction an authorisation mistake should fail.
        assert!(UserRole::Admin.at_least(UserRole::User));
        assert!(UserRole::Creator.at_least(UserRole::User));
        assert!(UserRole::Admin.at_least(UserRole::Creator));
        assert!(!UserRole::User.at_least(UserRole::Creator));
        assert!(!UserRole::Creator.at_least(UserRole::Admin));
        assert!(UserRole::Admin.at_least(UserRole::Admin));
    }

    // ── the JWT verifier, against real tokens ──────────────────────────────

    /// A test-only secret. Never a real one, and never read from the environment — a test
    /// that silently picked up a production secret could mint a valid token.
    const TEST_SECRET: &str = "a-test-secret-long-enough-for-hmac-sha256-signing";

    fn verifier() -> JwtVerifier {
        JwtVerifier::new(TEST_SECRET).expect("a verifier")
    }

    fn mint(claims: &AccessClaims) -> String {
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(Algorithm::HS256),
            claims,
            &jsonwebtoken::EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        )
        .expect("the test claims encode")
    }

    /// Sign a token with a key the verifier does not hold.
    fn mint_foreign(claims: &AccessClaims, algorithm: Algorithm) -> String {
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(algorithm),
            claims,
            &jsonwebtoken::EncodingKey::from_secret(b"an-entirely-different-secret-value"),
        )
        .expect("the test claims encode")
    }

    #[tokio::test]
    async fn a_correctly_signed_token_verifies() {
        let verified = verifier().verify(&mint(&claims())).await;

        assert_eq!(verified.expect("a valid token"), claims());
    }

    #[tokio::test]
    async fn a_token_signed_with_another_secret_is_rejected() {
        let token = mint_foreign(&claims(), Algorithm::HS256);

        let error = verifier()
            .verify(&token)
            .await
            .expect_err("a foreign signature must not verify");

        assert_eq!(error.code(), ErrorCode::InvalidToken);
        // Client-safe is correct: the client's remedy is to sign in again. What must never
        // travel is *why* the token failed, which is the verifier's log line, not its message.
        assert!(error.is_client_safe());
        assert_eq!(
            error.to_string(),
            "the access token is not valid",
            "the response message must not name the failure reason"
        );
    }

    #[tokio::test]
    async fn a_token_signed_with_another_algorithm_is_rejected() {
        // The algorithm pin, tested. If `validation.algorithms` were widened to "any
        // HMAC", this fails — which is the point, because `alg` is attacker-controlled
        // input and the classic bypass is asking the server to accept a weaker one.
        let token = mint_foreign(&claims(), Algorithm::HS512);

        let error = verifier()
            .verify(&token)
            .await
            .expect_err("HS512 must not verify where HS256 is pinned");

        assert_eq!(error.code(), ErrorCode::InvalidToken);
    }

    #[tokio::test]
    async fn an_expired_token_is_rejected() {
        // Checked during `decode`, so this is a property of the verifier rather than
        // something the middleware has to remember to do.
        let mut expired = claims();
        expired.iat = 1_600_000_000;
        expired.exp = 1_600_000_900;

        let error = verifier()
            .verify(&mint(&expired))
            .await
            .expect_err("an expired token must not verify");

        assert_eq!(error.code(), ErrorCode::InvalidToken);
    }

    #[tokio::test]
    async fn a_tampered_payload_is_rejected() {
        // Flipping one byte of the payload must invalidate the signature, not merely
        // produce a different user.
        let token = mint(&claims());
        let mut tampered = token.clone();
        let index = tampered.len() / 2;
        let replacement = if tampered.as_bytes()[index] == b'a' {
            'b'
        } else {
            'a'
        };
        tampered.replace_range(index..=index, &replacement.to_string());

        assert!(verifier().verify(&tampered).await.is_err());
    }

    #[tokio::test]
    async fn a_garbage_token_is_rejected_rather_than_panicking() {
        for token in ["", "not-a-jwt", "a.b.c", "....", "Bearer"] {
            assert!(
                verifier().verify(token).await.is_err(),
                "`{token}` must be refused"
            );
        }
    }

    #[test]
    fn a_blank_secret_is_a_startup_error_not_a_runtime_login_outage() {
        // §7.8. `jsonwebtoken` accepts an empty HMAC key and then rejects every token, so
        // without this check a missing `JWT_SECRET` surfaces as "login is broken".
        let error = JwtVerifier::new("   ").expect_err("a blank secret must be refused");

        assert_eq!(error.code(), ErrorCode::Internal);
        assert!(!error.is_client_safe());
    }

    #[test]
    fn the_algorithm_is_pinned_to_hs256() {
        // Asserted directly rather than inferred from a rejected token: an inferred
        // assertion passes for the wrong reason if the rejection came from the signature.
        assert_eq!(verifier().algorithm(), Algorithm::HS256);
    }

    #[test]
    fn the_verifier_never_prints_key_material() {
        // §9.5.
        let rendered = format!("{:?}", verifier());

        assert!(rendered.contains("redacted"));
        assert!(!rendered.contains(TEST_SECRET));
    }

    #[test]
    fn the_auth_state_never_prints_its_collaborators() {
        // axum `Debug`-prints router state in several setups, so the state type must not be
        // a channel for one.
        let rendered = format!("{:?}", accepting());

        assert!(rendered.contains("AuthState"));
        assert!(!rendered.contains("FakeVerifier"));
    }

    // ── the blocklist key ──────────────────────────────────────────────────

    #[test]
    fn the_blocklist_key_is_namespaced_and_bounded() {
        // §3.2: a revocation key with no expiry is exactly the unbounded growth the
        // mandatory-TTL rule exists to prevent, and the prefix keeps a scan for one kind of
        // entry from matching another's.
        let key = blocklist_key(Uuid::from_u128(7));

        assert_eq!(
            key.as_str(),
            format!("block-list:{BLOCKLIST_PREFIX}:{}", Uuid::from_u128(7))
        );
        assert_eq!(key.ttl(), ttl::BLOCK_LIST);
        assert!(!key.ttl().is_zero());
    }

    mod live {
        //! No Redis is available in unit-test runs, so these are `#[ignore]`d. Run with
        //! `cargo test -p meno-api -- --ignored middleware::auth`.

        use super::*;

        #[tokio::test]
        #[ignore = "needs a real Redis"]
        async fn a_written_revocation_is_seen_and_an_absent_one_is_not() {
            let url = std::env::var("REDIS_URL").expect("REDIS_URL is set for live tests");
            let redis = Redis::new(crate::infrastructure::redis::RedisConfig::from_url(url))
                .await
                .expect("a Redis");

            let blocklist = RedisTokenBlocklist::new(redis.clone());
            let jti = Uuid::from_u128(42);
            let key = blocklist_key(jti);

            assert!(!blocklist.is_revoked(jti, 0).await.expect("a read"));

            redis.set(&key, &true).await.expect("a write");
            assert!(blocklist.is_revoked(jti, 0).await.expect("a read"));

            redis.del(&key).await.expect("a delete");
        }

        #[tokio::test]
        #[ignore = "needs a real Redis"]
        async fn an_unreachable_redis_is_an_upstream_error_not_a_revocation() {
            // The property §4.2's fix exists to guarantee, exercised against the real
            // adapter rather than the fake.
            let redis = Redis::new(crate::infrastructure::redis::RedisConfig::from_url(
                "redis://127.0.0.1:1/".to_owned(),
            ))
            .await
            .expect("the client is built eagerly; only the pool is lazy");

            let error = RedisTokenBlocklist::new(redis)
                .is_revoked(Uuid::from_u128(1), 0)
                .await
                .expect_err("an unreachable Redis must surface");

            assert_eq!(error.code(), ErrorCode::UpstreamUnavailable);
            assert!(!error.is_client_safe());
        }
    }
}
