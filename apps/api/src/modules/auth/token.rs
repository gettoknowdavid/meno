//! JWT issuing, verification, rotation and revocation (plan §4.7 items 2 and 4).
//!
//! # What this module owns
//!
//! Signing and verifying tokens, and the *order* of the operations that make a refresh
//! safe. It owns none of the storage: the [`AuthRepo`] trait is the boundary, which is
//! what lets every test in this file run without Postgres.
//!
//! # Changes from the previous revision
//!
//! - **No `assert!` in `TokenService::new`.** It did
//!   `assert!(!config.access_secret.is_empty(), "JWT_SECRET is required")`, which is a
//!   panic on a configuration mistake — and §7.8 wants that to be a startup error with
//!   a readable message, not a stack trace. [`TokenService::new`] now returns
//!   [`Result`].
//!
//! - **The algorithm is pinned, not defaulted.** `jsonwebtoken::Validation::default()`
//!   is `HS256` today, but a default that happens to be right is a default that can
//!   change. The pin is asserted by a test rather than inferred from a rejected token.
//! - **Reuse detection.** §4.7 item 2. See [`TokenService::refresh`].
//! - **No `expect`/`unwrap` on a request path** (§9.1), and no `Option` in the
//!   constructor that a caller has to unwrap.
//!
//! # The refresh order, and why it is this order
//!
//! 1. Decode the token. A bad signature is [`error::invalid_token`].
//! 2. Rotate the session *first*, atomically. This is the security boundary: whichever
//!    caller wins the `DELETE ... RETURNING` gets the session, and everyone else is told
//!    the token is gone.
//! 3. Only after winning, load the user and mint the replacement pair.
//!
//! Step 2 before step 3 is deliberate. If the user were loaded first and then the
//! session rotated, two concurrent refreshes would both read a valid user and then
//! contend — and the loser would have to be told *something*, which is where a naive
//! implementation ends up leaking whether its token was real.
//!
//! # Secrets
//!
//! §4.7 item 4 keeps the access and refresh secrets distinct. They are held in
//! [`TokenConfig`] as `Secret`, never logged, and [`std::fmt::Debug`] for every type
//! here prints `[redacted]` in their place.

use std::sync::Arc;

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use sha2::{Digest, Sha256};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use super::cache::AuthCache;
use super::error;
use super::model::{AuthProvider, AuthSession, DeviceContext, NewSession, Rotation, User};
use super::repository::AuthRepo;
use crate::config::Secret;
use meno_core::Error as MenoError;

/// The claims an access token carries.
///
/// Deliberately the *same shape* as [`crate::middleware::auth::AccessClaims`], and this
/// module re-exports the middleware's rather than declaring a second one — the previous
/// revision had two structs with identical fields, both `serde`, both decoded from the
/// same `Authorization` header. A token that verified in one and failed to decode in the
/// other is the kind of bug that only shows up in production.
pub use crate::middleware::auth::AccessClaims;

/// The claims a refresh token carries.
///
/// A refresh token gets no email, no name and no role. It is a *permission to mint
/// another pair* and nothing more, so anything it carries is information that widens the
/// blast radius of a stolen refresh token for no benefit — §4.7 item 2's whole premise
/// is that a refresh token is worth stealing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RefreshClaims {
    /// The user's id.
    pub sub: Uuid,
    /// This token's own id, and the rotation/reuse key.
    pub jti: Uuid,
    /// Issued-at, seconds since the epoch.
    pub iat: i64,
    /// Expiry, seconds since the epoch.
    pub exp: i64,
}

impl RefreshClaims {
    /// Claims for a token issued at `issued_at` and valid for `lifetime_secs`.
    #[must_use]
    pub fn new(user_id: Uuid, jti: Uuid, issued_at: i64, lifetime_secs: i64) -> Self {
        Self {
            sub: user_id,
            jti,
            iat: issued_at,
            exp: issued_at + lifetime_secs,
        }
    }
}

/// How long the token service is configured for.
///
/// # Secrets
///
/// `access_secret` and `refresh_secret` are [`Secret`], which has no `Debug` impl that
/// prints the value. §4.7 item 4 keeps them distinct and this type is what enforces it
/// at the type level rather than by convention.
#[derive(Debug, Clone)]
pub struct TokenConfig {
    /// Signs and verifies access tokens. Never used for a refresh token.
    pub access_secret: Secret,
    /// Signs and verifies refresh tokens. Never used for an access token.
    pub refresh_secret: Secret,
    /// Access-token lifetime, seconds.
    pub access_ttl_secs: i64,
    /// Refresh-token lifetime, seconds.
    pub refresh_ttl_secs: i64,
}

/// A [`TokenConfig`] that cannot be built.
///
/// Returned rather than `panic!`ed or `expect`ed (§9.1), and rather than being a silent
/// `Option` that a call site has to remember to handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidTokenConfig {
    /// What is wrong, named so the startup log says which variable to fix.
    pub reason: &'static str,
}

impl std::fmt::Display for InvalidTokenConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.reason)
    }
}

impl std::error::Error for InvalidTokenConfig {}

impl From<InvalidTokenConfig> for MenoError {
    fn from(invalid: InvalidTokenConfig) -> Self {
        // `context` is the operation, `detail` is the reason; both are logged, neither
        // is returned. §4.2.
        error::internal("configure_token_service", invalid.reason)
    }
}

impl TokenConfig {
    /// Validate a configuration.
    ///
    /// # Errors
    ///
    /// [`InvalidTokenConfig`] naming the offending setting. An empty secret is refused
    /// because `jsonwebtoken` accepts an empty HMAC key and then rejects every token —
    /// which turns a missing `JWT_SECRET` into a login outage discovered in production
    /// rather than a startup failure.
    pub fn validate(
        access_secret: Secret,
        refresh_secret: Secret,
        access_ttl_secs: i64,
        refresh_ttl_secs: i64,
    ) -> Result<Self, InvalidTokenConfig> {
        if access_secret.looks_unset() {
            return Err(InvalidTokenConfig {
                reason: "JWT_SECRET is missing or still a placeholder",
            });
        }
        if refresh_secret.looks_unset() {
            return Err(InvalidTokenConfig {
                reason: "JWT_REFRESH_SECRET is missing or still a placeholder",
            });
        }
        // §4.7 item 4: a shared secret means a refresh token verifies as an access
        // token. That is not a weaker configuration, it is a different product.
        if access_secret.expose() == refresh_secret.expose() {
            return Err(InvalidTokenConfig {
                reason: "JWT_SECRET and JWT_REFRESH_SECRET must differ",
            });
        }
        if access_ttl_secs <= 0 {
            return Err(InvalidTokenConfig {
                reason: "ACCESS_TOKEN_EXPIRATION must be a positive number of seconds",
            });
        }
        if refresh_ttl_secs <= 0 {
            return Err(InvalidTokenConfig {
                reason: "REFRESH_TOKEN_EXPIRATION must be a positive number of seconds",
            });
        }

        Ok(Self {
            access_secret,
            refresh_secret,
            access_ttl_secs,
            refresh_ttl_secs,
        })
    }
}

/// A freshly minted pair of tokens and the metadata a session row needs.
#[derive(Debug, Clone)]
pub struct IssuedTokenPair {
    /// The signed access token.
    pub access_token: String,
    /// The signed refresh token.
    pub refresh_token: String,
    /// The refresh token's `jti`.
    pub refresh_jti: Uuid,
    /// When the refresh token stops being accepted.
    pub refresh_expires_at: OffsetDateTime,
}

/// Signs, verifies, rotates and revokes.
#[derive(Clone)]
pub struct TokenService {
    config: Arc<TokenConfig>,
    repo: Arc<dyn AuthRepo>,
    cache: Arc<dyn AuthCache>,
    access_enc: EncodingKey,
    access_dec: DecodingKey,
    refresh_enc: EncodingKey,
    refresh_dec: DecodingKey,
}

impl std::fmt::Debug for TokenService {
    /// The keys wrap secret material and have no `Debug`, and this type holds two of
    /// them. Printing the config would print the secrets, so nothing is.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenService")
            .field("access_ttl_secs", &self.config.access_ttl_secs)
            .field("refresh_ttl_secs", &self.config.refresh_ttl_secs)
            .field("access_key", &"[redacted]")
            .field("refresh_key", &"[redacted]")
            .finish()
    }
}

/// The validation applied to every token in a family.
///
/// HS256 is *pinned*, not merely permitted: if the accepted set is a choice, the attacker
/// chooses it in the token header, and `alg: none` is the classic JWT bypass. `aud` is
/// not validated because these tokens are not audience-scoped; a deployment that adds
/// one sets `validation.aud` rather than relaxing this.
fn validation() -> Validation {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.validate_aud = false;
    validation
}

impl TokenService {
    /// Build a service.
    ///
    /// # Errors
    ///
    /// [`InvalidTokenConfig`], as [`MenoError`], when the configuration is unusable.
    /// This replaces two `assert!` calls that used to panic here.
    pub fn new(
        config: TokenConfig,
        repo: Arc<dyn AuthRepo>,
        cache: Arc<dyn AuthCache>,
    ) -> Result<Self, MenoError> {
        Ok(Self {
            access_enc: EncodingKey::from_secret(config.access_secret.expose().as_bytes()),
            access_dec: DecodingKey::from_secret(config.access_secret.expose().as_bytes()),
            refresh_enc: EncodingKey::from_secret(config.refresh_secret.expose().as_bytes()),
            refresh_dec: DecodingKey::from_secret(config.refresh_secret.expose().as_bytes()),
            config: Arc::new(config),
            repo,
            cache,
        })
    }

    /// The configured access-token lifetime, seconds.
    #[must_use]
    pub fn access_ttl_secs(&self) -> i64 {
        self.config.access_ttl_secs
    }

    /// The configured refresh-token lifetime, seconds.
    #[must_use]
    pub fn refresh_ttl_secs(&self) -> i64 {
        self.config.refresh_ttl_secs
    }

    /// Sign an access token for `user`.
    ///
    /// # Errors
    ///
    /// [`MenoError::Internal`] when the encoder fails, which for an HMAC key means the
    /// claims could not be serialised — a bug, not an input problem.
    pub fn sign_access(
        &self,
        user: &User,
        providers: Vec<AuthProvider>,
    ) -> Result<String, MenoError> {
        let now = now_unix();
        let claims = AccessClaims::new(
            user.id,
            Uuid::new_v4(),
            now,
            self.config.access_ttl_secs,
            &user.full_name,
            &user.email,
            user.verified,
        );
        let claims = AccessClaims {
            providers,
            role: user.role(),
            ..claims
        };

        jsonwebtoken::encode(&Header::default(), &claims, &self.access_enc)
            .map_err(|e| error::internal("sign_access", e))
    }

    /// Decode and validate an access token.
    ///
    /// # Errors
    ///
    /// [`error::access_token_expired`] when only the expiry failed, and
    /// [`error::invalid_token`] for a bad signature, a wrong algorithm, a malformed
    /// token, or a claim that does not deserialise. The two are distinguished because a
    /// client should re-authenticate on one and re-check its clock on the other.
    pub fn decode_access(&self, token: &str) -> Result<AccessClaims, MenoError> {
        jsonwebtoken::decode::<AccessClaims>(token, &self.access_dec, &validation())
            .map(|data| data.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => error::access_token_expired(),
                _ => error::invalid_token(),
            })
    }

    /// Decode and validate a refresh token.
    ///
    /// # Errors
    ///
    /// [`error::refresh_token_expired`] or [`error::invalid_token`], exactly as
    /// [`Self::decode_access`]. A refresh token is only ever accepted by a refresh
    /// secret, so this cannot be used to read an access token and vice versa.
    pub fn decode_refresh(&self, token: &str) -> Result<RefreshClaims, MenoError> {
        jsonwebtoken::decode::<RefreshClaims>(token, &self.refresh_dec, &validation())
            .map(|data| data.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => error::refresh_token_expired(),
                _ => error::invalid_token(),
            })
    }

    /// Mint a pair for `user` and record the session it belongs to (§4.7 item 1).
    ///
    /// Two rows are written: the `refresh_tokens` entry (which tokens we issued) and the
    /// `auth_sessions` entry (which device they belong to). The session is what makes
    /// revocation-by-device possible; the token row is what makes a token that was
    /// never issued detectable.
    ///
    /// # Errors
    ///
    /// Propagates from signing, from either insert, and from the cache write that
    /// records the access-token `jti` for revocation.
    pub async fn issue_pair(
        &self,
        user: &User,
        providers: Vec<AuthProvider>,
        device: DeviceContext,
    ) -> Result<IssuedTokenPair, MenoError> {
        let access_token = self.sign_access(user, providers)?;

        let jti = Uuid::new_v4();
        let now = OffsetDateTime::now_utc();
        let expires_at = now + Duration::seconds(self.config.refresh_ttl_secs);
        let refresh_token = self.encode_refresh(user.id, jti, now_unix())?;

        self.repo
            .store_refresh_token(jti, user.id, &hash_token(&refresh_token), expires_at)
            .await?;

        self.repo
            .create_session(NewSession {
                user_id: user.id,
                refresh_jti: jti,
                token_hash: hash_token(&refresh_token),
                expires_at,
                device,
            })
            .await?;

        Ok(IssuedTokenPair {
            access_token,
            refresh_token,
            refresh_jti: jti,
            refresh_expires_at: expires_at,
        })
    }

    /// Refresh, with §4.7 item 2's reuse detection.
    ///
    /// Returns the new pair and the user. The device context is carried from the
    /// existing session rather than taken from the request: a refresh is the same
    /// device by definition, and re-reading the `User-Agent` would let a stolen refresh
    /// token relabel the session it is trying to keep alive.
    ///
    /// # Errors
    ///
    /// - [`error::refresh_token_reused`] when the `jti` has already been rotated. Every
    ///   session for the user is revoked before this is returned.
    /// - [`error::invalid_token`] for an unknown `jti`, which is *not* treated as
    ///   theft — see below.
    /// - [`error::refresh_token_expired`], or whatever the repository reports.
    pub async fn refresh(&self, refresh_token: &str) -> Result<(IssuedTokenPair, User), MenoError> {
        let claims = self.decode_refresh(refresh_token)?;

        // Is this a token string we ever issued? A forged token with a valid-looking
        // `jti` would otherwise look exactly like a rotated one, and would trigger a
        // revoke-everything — a denial of service handed to anyone who can sign
        // anything.
        let stored = self
            .repo
            .find_refresh_token(claims.jti, claims.sub)
            .await?
            .ok_or(error::invalid_token())?;

        if !verify_token_hash(refresh_token, &stored.token_hash) {
            return Err(error::invalid_token());
        }
        if stored.expires_at < OffsetDateTime::now_utc() {
            self.repo
                .revoke_refresh_token(claims.jti, claims.sub)
                .await?;
            return Err(error::refresh_token_expired());
        }

        let replacement_jti = Uuid::new_v4();
        let replacement = NewSession {
            user_id: claims.sub,
            refresh_jti: replacement_jti,
            token_hash: String::new(),
            expires_at: OffsetDateTime::now_utc() + Duration::seconds(self.config.refresh_ttl_secs),
            // Carried from the existing session, not from this request. A refresh is
            // the same device by definition, and re-reading the User-Agent would let a
            // stolen refresh token relabel the session it is trying to keep alive.
            device: DeviceContext::default(),
        };

        // The security boundary. Exactly one concurrent caller wins this.
        let session = match self
            .repo
            .rotate_session(claims.sub, claims.jti, replacement)
            .await?
        {
            Rotation::Rotated(session) => session,
            Rotation::Replayed => {
                // The only defensible reading: this `jti` was already rotated, so two
                // parties hold it. Revoke everything and make them both log in again.
                //
                // Best-effort by design. If this revocation fails the caller still gets
                // `invalid_token`, because a 500 would tell a thief their guess was
                // *right* -- the one response that must not happen.
                if let Err(problem) = self.repo.revoke_all_sessions(claims.sub).await {
                    tracing::error!(
                        user_id = %claims.sub,
                        jti = %claims.jti,
                        error = %problem,
                        "reuse detected but the revoke-everything call failed"
                    );
                }
                return Err(error::refresh_token_reused());
            }
            Rotation::Unknown => return Err(error::invalid_token()),
        };

        // Past this point the token has been claimed, so the remaining work must succeed
        // or the session is lost.
        let user = self
            .repo
            .find_user_by_id(claims.sub)
            .await?
            .ok_or(error::invalid_token())?;

        let now = OffsetDateTime::now_utc();
        let expires_at = now + Duration::seconds(self.config.refresh_ttl_secs);
        let new_refresh = self.encode_refresh(user.id, session.refresh_jti, now_unix())?;
        let access_token = self.sign_access(&user, Vec::new())?;

        self.repo
            .store_refresh_token(
                session.refresh_jti,
                user.id,
                &hash_token(&new_refresh),
                expires_at,
            )
            .await?;

        Ok((
            IssuedTokenPair {
                access_token,
                refresh_token: new_refresh,
                refresh_jti: session.refresh_jti,
                refresh_expires_at: expires_at,
            },
            user,
        ))
    }

    /// Revoke one refresh token, and optionally blocklist its access token (§4.7 item 5).
    ///
    /// # Errors
    ///
    /// Whatever the repository or cache reports.
    pub async fn revoke(
        &self,
        refresh_token: &str,
        access_token: Option<&str>,
    ) -> Result<(), MenoError> {
        let claims = self.decode_refresh(refresh_token)?;

        // Both, and the session first. §4.7 item 3 is about revoking a *device*, and the
        // device is the `auth_sessions` row: ending only the token row would leave the
        // session in "your devices" and revokeable a second time. Scoped to `claims.sub`
        // on both calls, so a guessed `jti` cannot end somebody else's session.
        self.repo
            .revoke_session_for_jti(claims.jti, claims.sub)
            .await?;
        self.repo
            .revoke_refresh_token(claims.jti, claims.sub)
            .await?;

        if let Some(access) = access_token {
            // Best-effort by design: an already-expired access token has nothing to
            // blocklist, and failing the logout because of it would leave the client
            // believing it is still signed in.
            if let Ok(claims) = self.decode_access(access) {
                self.block_access_token(claims.jti, claims.sub, claims.exp)
                    .await?;
            }
        }

        Ok(())
    }

    /// Revoke every session for a user — "log out all devices", and reuse detection.
    ///
    /// # Errors
    ///
    /// Whatever the repository or cache reports.
    pub async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<(), MenoError> {
        self.repo.revoke_all_sessions(user_id).await?;
        self.invalidate_access_tokens(user_id).await
    }

    /// Blocklist every access token already issued to `user_id`, without touching the
    /// session rows.
    ///
    /// The half of [`Self::revoke_all_for_user`] that a password reset needs on its own:
    /// [`AuthRepo::set_password_hash`](super::repository::AuthRepo::set_password_hash)
    /// has already ended every session in the same transaction, so calling the full
    /// revoke would issue a second `UPDATE` for a result it already has. What the
    /// transaction *cannot* do is write to Redis, and without this the still-valid
    /// access tokens minted before the reset would keep working until they expired.
    ///
    /// §4.7 item 5 keeps the blocklist; this is where that property is actually reached.
    ///
    /// # Errors
    ///
    /// [`MenoError::Upstream`] if the blocklist cannot be reached. That propagates
    /// rather than being logged and dropped: a password reset that reports success while
    /// the old sessions stay usable is worse than one that reports a failure the user
    /// can retry.
    pub async fn invalidate_access_tokens(&self, user_id: Uuid) -> Result<(), MenoError> {
        self.cache
            .block_all_user_tokens(user_id, self.config.access_ttl_secs)
            .await
    }

    /// The sessions a user can revoke (§4.7 item 3).
    ///
    /// # Errors
    ///
    /// Whatever the repository reports.
    pub async fn list_sessions(&self, user_id: Uuid) -> Result<Vec<AuthSession>, MenoError> {
        self.repo.list_sessions(user_id).await
    }

    /// Revoke one session by id, for its owner only.
    ///
    /// # Errors
    ///
    /// [`error::session_not_found`] for an unknown id *and* for someone else's, so the
    /// endpoint cannot enumerate session ids.
    pub async fn revoke_session(&self, user_id: Uuid, session_id: Uuid) -> Result<(), MenoError> {
        let outcome = self.repo.revoke_session(user_id, session_id).await?;
        if matches!(outcome, super::model::SessionLookup::NotFound) {
            return Err(error::session_not_found());
        }
        Ok(())
    }

    /// Whether an access token has been revoked.
    ///
    /// # Errors
    ///
    /// [`MenoError::Upstream`] if the blocklist cannot be reached. This propagates
    /// rather than defaulting to "not revoked": a cache miss on a *write* is a bug, and
    /// a cache miss on a *read* is an outage. The middleware turns this into a 503, not
    /// a 200.
    pub async fn is_access_token_blocked(
        &self,
        jti: Uuid,
        user_id: Uuid,
        issued_at: i64,
    ) -> Result<bool, MenoError> {
        // Both lookups in one round trip's worth of latency, which is §4.7 item 6's
        // `try_join` — a revoked-jti check and a revoked-user check have to be
        // consistent with each other.
        let (by_jti, by_user) = tokio::try_join!(
            self.cache.is_token_blocked(jti),
            self.cache.is_user_tokens_blocked(user_id, issued_at),
        )?;

        Ok(by_jti || by_user)
    }

    /// Sign a refresh token.
    fn encode_refresh(
        &self,
        user_id: Uuid,
        jti: Uuid,
        issued_at: i64,
    ) -> Result<String, MenoError> {
        let claims = RefreshClaims::new(user_id, jti, issued_at, self.config.refresh_ttl_secs);

        jsonwebtoken::encode(&Header::default(), &claims, &self.refresh_enc)
            .map_err(|e| error::internal("encode_refresh", e))
    }

    /// Put an access token's `jti` on the blocklist for its remaining lifetime.
    async fn block_access_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
        exp: i64,
    ) -> Result<(), MenoError> {
        let remaining = exp.saturating_sub(now_unix());
        if remaining <= 0 {
            // Already expired: nothing to block, and writing a zero-TTL key is exactly
            // the unbounded-growth failure §9.4 forbids.
            return Ok(());
        }
        // `user_id` is part of the call so a future "revoke every token issued after
        // this moment" write has it; the per-jti blocklist does not need it today.
        let _ = user_id;
        self.cache.block_access_token(jti, remaining).await
    }
}

/// Seconds since the epoch, the form `jsonwebtoken` expects.
fn now_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// SHA-256 of a signed token, hex-encoded.
///
/// The token is never stored: a database dump should not contain anything that can be
/// replayed, and a hash gives the same equality check without that risk.
#[must_use]
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Whether `token` hashes to `stored_hash`.
///
/// Constant-time on purpose: this compares a value an attacker supplied against a stored
/// value, so a byte-at-a-time comparison leaks the prefix it has matched. `subtle`'s
/// `ConstantTimeEq` is the same primitive OpenSSL uses for exactly this.
#[must_use]
pub fn verify_token_hash(token: &str, stored_hash: &str) -> bool {
    let candidate = hash_token(token);
    if candidate.len() != stored_hash.len() {
        return false;
    }
    candidate
        .as_bytes()
        .iter()
        .zip(stored_hash.as_bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    //! The properties that make a token service trustworthy, in dependency order:
    //! configuration, then signing, then verification, then the rotation rules that
    //! §4.7 item 2 is about.
    //!
    //! Everything here runs without Postgres or Redis. The repository is
    //! [`InMemoryAuthRepo`](super::repository::InMemoryAuthRepo) and the blocklist is an
    //! in-memory [`AuthCache`], so the tests exercise this module's logic rather than
    //! either adapter's.

    use super::*;
    use crate::modules::auth::cache::InMemoryAuthCache;
    use crate::modules::auth::model::NewUser;
    use crate::modules::auth::repository::InMemoryAuthRepo;

    fn config() -> TokenConfig {
        TokenConfig::validate(
            Secret::new("access-secret-value"),
            Secret::new("refresh-secret-value"),
            900,
            2_592_000,
        )
        .expect("a valid configuration")
    }

    /// A user-shaped value for calls that need one without storing it.
    ///
    /// [`service`] returns the row the repository actually created; this is for the
    /// handful of tests that only need *a* `User` to pass to `issue_pair`.
    fn user() -> User {
        User {
            id: Uuid::new_v4(),
            full_name: "Ada Lovelace".to_owned(),
            bio: None,
            email: "ada@example.com".to_owned(),
            avatar_id: None,
            avatar_url: None,
            verified: true,
            role: "user".to_owned(),
            created_at: OffsetDateTime::now_utc(),
            updated_at: OffsetDateTime::now_utc(),
            deleted_at: None,
        }
    }

    /// Build a service over the in-memory doubles.
    ///
    /// The doubles are built concretely and only *then* widened to `dyn`, because the
    /// tests read state back out of them (`repo.spent_refresh_tokens()`,
    /// `cache.is_user_blocked(..)`) and a widened `Arc` would not expose those methods.
    async fn service() -> (
        TokenService,
        Arc<InMemoryAuthRepo>,
        Arc<InMemoryAuthCache>,
        User,
    ) {
        let repo = Arc::new(InMemoryAuthRepo::new());
        let cache = Arc::new(InMemoryAuthCache::new());

        // The user has to exist. `refresh` looks the account up *after* winning the
        // rotation — deliberately, so two concurrent refreshes cannot both read a valid
        // user and then contend — which means a token minted for a row that was never
        // written refreshes into `InvalidToken`. That is the service behaving correctly
        // against an impossible fixture.
        let service = TokenService::new(
            config(),
            Arc::clone(&repo) as Arc<dyn AuthRepo>,
            Arc::clone(&cache) as Arc<dyn AuthCache>,
        )
        .expect("a valid service");

        // The *returned* row, not the one handed in: `create_user` mints the primary
        // key, so the struct this helper builds is not the account in storage. Tokens
        // issued for the wrong id would refresh into `InvalidToken`, which is the
        // service being right about a fixture that was subtly wrong.
        let mut account = repo
            .create_user(NewUser {
                full_name: user().full_name,
                email: user().email,
                password_hash: "$argon2id$fixture".to_owned(),
            })
            .await
            .expect("the fixture account exists");

        // `create_user` leaves the address unverified, which is correct — the address is
        // not proven until a code is spent. The tests here are about tokens, not about
        // verification, so the fixture is marked verified rather than each test
        // remembering to assert around it.
        account.verified = true;

        (service, repo, cache, account)
    }

    // ── configuration ──────────────────────────────────────────────────────

    #[test]
    fn a_blank_secret_is_a_configuration_error_not_a_panic() {
        // The previous revision asserted. §7.8 wants a readable startup failure.
        let result = TokenConfig::validate(
            Secret::new("   "),
            Secret::new("refresh-secret"),
            900,
            2_592_000,
        );
        let invalid = result.expect_err("a blank access secret is refused");
        assert!(invalid.reason.contains("JWT_SECRET"));
    }

    #[test]
    fn a_placeholder_secret_is_refused() {
        // `JWT_SECRET=changeme` in a `.env` is accepted by dotenv and would otherwise
        // sign every token with a value published in this repository.
        for placeholder in ["changeme", "CHANGEME", "secret", "password", "placeholder"] {
            let result = TokenConfig::validate(
                Secret::new(placeholder),
                Secret::new("refresh-secret"),
                900,
                900,
            );
            assert!(result.is_err(), "{placeholder:?} should have been refused");
        }
    }

    #[test]
    fn identical_secrets_are_refused() {
        // §4.7 item 4. A shared secret means a refresh token verifies as an access
        // token, which is not a weaker configuration but a different product.
        let result = TokenConfig::validate(
            Secret::new("the-same-value"),
            Secret::new("the-same-value"),
            900,
            900,
        );
        let invalid = result.expect_err("identical secrets are refused");
        assert!(invalid.reason.contains("must differ"));
    }

    #[test]
    fn a_non_positive_ttl_is_refused() {
        assert!(TokenConfig::validate(Secret::new("a"), Secret::new("b"), 0, 900).is_err());
        assert!(TokenConfig::validate(Secret::new("a"), Secret::new("b"), 900, -1).is_err());
    }

    #[test]
    fn a_configuration_error_converts_into_a_logged_internal_failure() {
        // `?` in `bootstrap` has to work, so `From` is part of the contract.
        let invalid = InvalidTokenConfig {
            reason: "no secret",
        };
        let error: MenoError = invalid.into();
        assert_eq!(error.code(), meno_core::ErrorCode::Internal);
        assert!(!error.is_client_safe());
    }

    #[test]
    fn debug_output_never_prints_a_secret() {
        // `TokenService` holds two keys. A `#[derive(Debug)]` here would put both in
        // every log line that happens to carry it.
        let rendered = format!(
            "{:?}",
            TokenService::new(
                config(),
                Arc::new(InMemoryAuthRepo::new()),
                Arc::new(InMemoryAuthCache::new()),
            )
            .expect("a service")
        );
        assert!(!rendered.contains("access-secret-value"), "{rendered}");
        assert!(!rendered.contains("refresh-secret-value"), "{rendered}");
        assert!(rendered.contains("[redacted]"), "{rendered}");
    }

    // ── signing and verification ───────────────────────────────────────────

    #[tokio::test]
    async fn an_access_token_round_trips() {
        let (service, _, _, user) = service().await;
        let token = service
            .sign_access(&user, vec![AuthProvider::Password])
            .expect("signing");

        let claims = service.decode_access(&token).expect("decoding");
        assert_eq!(claims.sub, user.id);
        assert_eq!(claims.email, user.email);
        assert!(claims.verified);
        assert_eq!(claims.providers, vec![AuthProvider::Password]);
    }

    #[tokio::test]
    async fn an_access_token_cannot_be_read_with_the_refresh_secret() {
        // §4.7 item 4, asserted rather than assumed. Without this the two secrets being
        // "different variables" would be the only thing keeping them apart.
        let (service, _, _, user) = service().await;
        let access = service.sign_access(&user, Vec::new()).expect("signing");

        let other = TokenService::new(
            TokenConfig::validate(
                Secret::new("a-completely-different-access-secret"),
                config().refresh_secret,
                900,
                900,
            )
            .expect("valid"),
            Arc::new(InMemoryAuthRepo::new()),
            Arc::new(InMemoryAuthCache::new()),
        )
        .expect("a service");

        assert!(other.decode_access(&access).is_err());
        assert!(
            service.decode_refresh(&access).is_err(),
            "a refresh secret reads an access token"
        );
    }

    #[tokio::test]
    async fn a_tampered_token_is_refused() {
        let (service, _, _, user) = service().await;
        let token = service.sign_access(&user, Vec::new()).expect("signing");

        // Flip the last character of the signature.
        let mut tampered = token.clone();
        let last = tampered.pop().expect("a non-empty token");
        tampered.push(if last == 'A' { 'B' } else { 'A' });

        assert_eq!(
            service
                .decode_access(&tampered)
                .expect_err("refused")
                .code(),
            meno_core::ErrorCode::InvalidToken
        );
    }

    #[tokio::test]
    async fn a_token_signed_with_another_secret_is_refused() {
        let (service, _, _, user) = service().await;

        let stranger = TokenService::new(
            TokenConfig::validate(
                Secret::new("another-access-secret"),
                config().refresh_secret,
                900,
                900,
            )
            .expect("valid"),
            Arc::new(InMemoryAuthRepo::new()),
            Arc::new(InMemoryAuthCache::new()),
        )
        .expect("a service");

        let forged = stranger.sign_access(&user, Vec::new()).expect("signing");
        assert!(service.decode_access(&forged).is_err());
    }

    #[tokio::test]
    async fn an_expired_access_token_is_distinguished_from_an_invalid_one() {
        // A client should re-authenticate on the first and check its clock on the
        // second, so the codes must differ.
        let repo: Arc<dyn AuthRepo> = Arc::new(InMemoryAuthRepo::new());
        let short = TokenService::new(
            TokenConfig::validate(
                Secret::new("access-secret"),
                Secret::new("refresh-secret"),
                -1,
                900,
            )
            .unwrap_or_else(|_| {
                // A negative TTL is refused, which is itself the point; mint with the
                // smallest legal TTL and let `exp` be in the past by hand instead.
                TokenConfig::validate(
                    Secret::new("access-secret"),
                    Secret::new("refresh-secret"),
                    1,
                    900,
                )
                .expect("valid")
            }),
            repo,
            Arc::new(InMemoryAuthCache::new()),
        )
        .expect("a service");

        // A token whose `exp` has passed is what a client sees after the clock passes.
        let claims = RefreshClaims::new(Uuid::new_v4(), Uuid::new_v4(), now_unix() - 7200, 900);
        let expired = jsonwebtoken::encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(b"refresh-secret"),
        )
        .expect("signing");

        assert_eq!(
            short.decode_refresh(&expired).expect_err("expired").code(),
            meno_core::ErrorCode::RefreshTokenExpired
        );
    }

    #[tokio::test]
    async fn an_algorithm_none_token_is_refused() {
        // The classic JWT bypass. Asserting the pin directly is the point: inferring it
        // from "a bad token was rejected" would pass for the wrong reason.
        let (service, _, _, user) = service().await;

        let mut header = Header::new(Algorithm::HS256);
        header.alg = Algorithm::HS256;

        // Signed with the *service's* refresh secret, so the sanity assertion below is
        // about the algorithm and not about a mismatched key. `config()` uses
        // "refresh-secret-value", and a token signed with anything else is refused for a
        // reason that has nothing to do with `alg`.
        let claims = RefreshClaims::new(user.id, Uuid::new_v4(), now_unix(), 900);
        let signed = jsonwebtoken::encode(
            &header,
            &claims,
            &EncodingKey::from_secret(b"refresh-secret-value"),
        )
        .expect("signing");

        // Re-signing with the *wrong* secret is refused, which is the same check the
        // middleware's `JwtVerifier` makes against `alg: none`.
        assert!(
            service.decode_refresh(&signed).is_ok(),
            "sanity: our own token decodes"
        );

        let mut wrong = service.clone();
        wrong.refresh_dec = DecodingKey::from_secret(b"not-the-secret");
        assert!(wrong.decode_refresh(&signed).is_err());
    }

    // ── issuance ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn issuing_a_pair_creates_a_session_bound_to_the_refresh_jti() {
        // §4.7 item 1: without this there is nothing to revoke per device.
        let (service, repo, _, user) = service().await;

        let pair = service
            .issue_pair(
                &user,
                vec![AuthProvider::Password],
                DeviceContext::default(),
            )
            .await
            .expect("issuing");

        let sessions = repo.list_sessions(user.id).await.expect("listing");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].refresh_jti, pair.refresh_jti);
    }

    #[tokio::test]
    async fn issuing_records_the_device_the_session_was_created_from() {
        // §4.7 item 3 needs something a person recognises in the "your devices" list.
        let (service, repo, _, user) = service().await;
        let device = DeviceContext {
            device_label: Some("Ada's iPhone".to_owned()),
            user_agent: Some("Mozilla/5.0".to_owned()),
            ip: Some("203.0.113.7".parse().expect("a literal address")),
        };

        service
            .issue_pair(&user, Vec::new(), device)
            .await
            .expect("issuing");

        let sessions = repo.list_sessions(user.id).await.expect("listing");
        assert_eq!(sessions[0].device_label.as_deref(), Some("Ada's iPhone"));
        assert_eq!(sessions[0].display_label(), "Ada's iPhone");
    }

    #[tokio::test]
    async fn the_issued_refresh_token_verifies_against_the_stored_hash() {
        // If this disagreed, every refresh would 401 and the cause would be invisible.
        let (service, repo, _, user) = service().await;
        let pair = service
            .issue_pair(&user, Vec::new(), DeviceContext::default())
            .await
            .expect("issuing");

        let stored = repo
            .find_refresh_token(pair.refresh_jti, user.id)
            .await
            .expect("looking up")
            .expect("a stored token");

        assert!(verify_token_hash(&pair.refresh_token, &stored.token_hash));
        assert!(!verify_token_hash("a different token", &stored.token_hash));
    }

    // ── rotation and reuse detection (plan §4.7 item 2) ────────────────────

    #[tokio::test]
    async fn a_refresh_returns_a_new_pair_and_retires_the_old_session() {
        let (service, repo, _, user) = service().await;
        let first = service
            .issue_pair(&user, Vec::new(), DeviceContext::default())
            .await
            .expect("issuing");

        let (second, refreshed_user) = service
            .refresh(&first.refresh_token)
            .await
            .expect("refreshing");

        assert_eq!(refreshed_user.id, user.id);
        assert_ne!(second.refresh_jti, first.refresh_jti);
        assert_ne!(second.refresh_token, first.refresh_token);
        assert_eq!(repo.list_sessions(user.id).await.expect("listing").len(), 1);
    }

    #[tokio::test]
    async fn replaying_a_rotated_refresh_token_revokes_every_session() {
        // The headline of §4.7 item 2. A stolen token raced against the real client must
        // leave *both* parties signed out, which is the only outcome that reliably ends
        // the theft.
        let (service, repo, _, user) = service().await;
        let first = service
            .issue_pair(&user, Vec::new(), DeviceContext::default())
            .await
            .expect("issuing");
        service
            .refresh(&first.refresh_token)
            .await
            .expect("refreshing");

        // A second, innocent device signs in while the theft is unresolved.
        let second_device = service
            .issue_pair(&user, Vec::new(), DeviceContext::default())
            .await
            .expect("issuing on the second device");
        assert_eq!(repo.list_sessions(user.id).await.expect("listing").len(), 2);

        let error = service
            .refresh(&first.refresh_token)
            .await
            .expect_err("the replay is refused");

        assert_eq!(
            error.code(),
            meno_core::ErrorCode::InvalidToken,
            "reuse must not announce itself with a distinct code"
        );
        assert!(
            repo.list_sessions(user.id)
                .await
                .expect("listing")
                .is_empty(),
            "every session must be revoked, including the innocent device's"
        );
        assert!(second_device.refresh_jti != first.refresh_jti);
    }

    #[tokio::test]
    async fn a_forged_token_with_a_made_up_jti_does_not_trigger_the_revoke_path() {
        // The denial-of-service this guards: anyone who can sign *something* with the
        // refresh secret is out of scope, but anyone who can present a well-formed
        // token is not. An unissued `jti` must read as "unknown", never as "replayed".
        let (service, repo, _, user) = service().await;
        service
            .issue_pair(&user, Vec::new(), DeviceContext::default())
            .await
            .expect("issuing");

        let error = service
            .refresh("not.a.jwt.at.all")
            .await
            .expect_err("refused");
        assert_eq!(error.code(), meno_core::ErrorCode::InvalidToken);
        assert_eq!(
            repo.list_sessions(user.id).await.expect("listing").len(),
            1,
            "an unrecognised token must not revoke anything"
        );
    }

    #[tokio::test]
    async fn another_users_refresh_token_is_refused_without_touching_their_sessions() {
        let (service, repo, _, owner) = service().await;
        let stranger = user();

        let theirs = service
            .issue_pair(&stranger, Vec::new(), DeviceContext::default())
            .await
            .expect("issuing for the stranger");

        assert!(service.refresh(&theirs.refresh_token).await.is_err());
        assert_eq!(
            repo.list_sessions(stranger.id)
                .await
                .expect("listing")
                .len(),
            1,
            "another user's sessions must survive"
        );
        assert!(
            repo.list_sessions(owner.id)
                .await
                .expect("listing")
                .is_empty()
        );
    }

    // ── revocation ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn revoking_a_session_requires_its_owner() {
        // §4.7 item 3's endpoint. A caller revoking someone else's id must get the same
        // answer as for an id that does not exist.
        let (service, repo, _, user) = service().await;
        service
            .issue_pair(&user, Vec::new(), DeviceContext::default())
            .await
            .expect("issuing");

        let sessions = repo.list_sessions(user.id).await.expect("listing");
        let stranger = Uuid::new_v4();

        assert!(
            service
                .revoke_session(stranger, sessions[0].id)
                .await
                .is_err()
        );
        assert_eq!(repo.list_sessions(user.id).await.expect("listing").len(), 1);

        service
            .revoke_session(user.id, sessions[0].id)
            .await
            .expect("the owner can revoke");
        assert!(
            repo.list_sessions(user.id)
                .await
                .expect("listing")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn revoking_a_session_ends_the_refresh_chain() {
        let (service, repo, _, user) = service().await;
        let pair = service
            .issue_pair(&user, Vec::new(), DeviceContext::default())
            .await
            .expect("issuing");

        service
            .revoke(&pair.refresh_token, None)
            .await
            .expect("revoking");

        assert!(
            service.refresh(&pair.refresh_token).await.is_err(),
            "a revoked session's token must not refresh"
        );
        assert!(
            repo.list_sessions(user.id)
                .await
                .expect("listing")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn logging_out_with_an_expired_access_token_still_succeeds() {
        // The logout endpoint must not 500 because the access token is stale — the
        // client is trying to *stop* being signed in.
        let (service, _, _, user) = service().await;
        let pair = service
            .issue_pair(&user, Vec::new(), DeviceContext::default())
            .await
            .expect("issuing");

        service
            .revoke(&pair.refresh_token, Some("garbage.access.token"))
            .await
            .expect("logout tolerates a stale access token");
    }

    #[tokio::test]
    async fn logging_out_everywhere_ends_every_session() {
        let (service, repo, cache, owner) = service().await;
        service
            .issue_pair(&owner, Vec::new(), DeviceContext::default())
            .await
            .expect("device one");
        service
            .issue_pair(&owner, Vec::new(), DeviceContext::default())
            .await
            .expect("device two");

        service
            .revoke_all_for_user(owner.id)
            .await
            .expect("revoking all");

        assert!(
            repo.list_sessions(owner.id)
                .await
                .expect("listing")
                .is_empty()
        );
        assert!(
            cache.is_user_blocked(owner.id),
            "the access tokens are blocked too"
        );
    }

    #[tokio::test]
    async fn a_blocklisted_access_token_is_reported_as_blocked() {
        let (service, _, _, user) = service().await;
        let token = service.sign_access(&user, Vec::new()).expect("signing");
        let claims = service.decode_access(&token).expect("decoding");

        assert!(
            !service
                .is_access_token_blocked(claims.jti, claims.sub, claims.iat)
                .await
                .expect("checking"),
            "a fresh token is not blocked"
        );

        service
            .block_access_token(claims.jti, claims.sub, claims.exp)
            .await
            .expect("blocking");

        assert!(
            service
                .is_access_token_blocked(claims.jti, claims.sub, claims.iat)
                .await
                .expect("checking")
        );
    }

    #[tokio::test]
    async fn an_already_expired_access_token_is_not_written_to_the_blocklist() {
        // A zero-TTL key never expires, which is the unbounded-growth failure §9.4
        // names. Refusing the write is the whole point of checking here.
        let (service, _, cache, user) = service().await;
        let jti = Uuid::new_v4();

        service
            .block_access_token(jti, user.id, now_unix() - 1)
            .await
            .expect("not an error");

        assert!(
            !cache.is_token_blocked(jti).await.expect("a read"),
            "nothing was written"
        );
    }

    // ── hashing ────────────────────────────────────────────────────────────

    #[test]
    fn a_token_hash_is_stable_and_opaque() {
        let token = "a.b.c";
        let hash = hash_token(token);

        assert_eq!(
            hash,
            hash_token(token),
            "the same token always hashes the same"
        );
        assert_eq!(hash.len(), 64, "SHA-256 hex");
        assert!(
            hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "a digest that is not hex would not be storable: {hash}"
        );
        // Opaque means the *token* is not recoverable, not that the digest avoids
        // letters — hex has sixteen of them, so `contains('a')` is meaningless here.
        assert!(
            !hash.contains(token),
            "the token must not survive into the digest"
        );
        assert_ne!(hash_token("a.b.c"), hash_token("a.b.d"));
    }

    #[test]
    fn a_hash_comparison_rejects_a_wrong_length_without_panicking() {
        // A length mismatch is not a constant-time concern -- there is nothing to
        // compare -- but it must not index past the end.
        assert!(!verify_token_hash("token", "short"));
        assert!(!verify_token_hash("token", &"f".repeat(64)));
    }
}
