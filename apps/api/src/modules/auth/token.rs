use crate::modules::auth::cache::AuthCache;
use crate::modules::auth::errors::AuthError;
use crate::modules::auth::model::{AuthProvider, User, UserRole};
use crate::modules::auth::repository::AuthRepo;
use jsonwebtoken::{DecodingKey, EncodingKey};
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct AccessClaims {
    pub sub: uuid::Uuid,
    pub jti: uuid::Uuid,
    pub email: String,
    pub full_name: String,
    pub verified: bool,
    pub providers: Vec<AuthProvider>,
    pub role: UserRole,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct RefreshClaims {
    pub sub: uuid::Uuid,
    pub jti: uuid::Uuid,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Clone)]
pub struct TokenConfig {
    /// The access token secret
    pub access_secret: String,

    /// The refresh token secret
    pub refresh_secret: String,

    /// Seconds until access token expires (e.g. 900)
    pub access_ttl_secs: i64,

    /// Seconds until refresh token expires (e.g. `2_592_000` = 30 days)
    pub refresh_ttl_secs: i64,
}

#[derive(Debug, Clone)]
pub struct IssuedTokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub refresh_jti: uuid::Uuid,

    /// When the refresh token row expires (stored in DB)
    pub refresh_expires_at: time::OffsetDateTime,
}

/// Owns all JWT encoding/decoding and orchestrates the full token lifecycle:
/// signing, verification, rotation, and revocation.
///
/// `R` and `C` are generic to allow test doubles. In production they are
/// `AuthRepository` and `RedisAuthCache`.
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
impl TokenService {
    pub fn new(config: TokenConfig, repo: Arc<dyn AuthRepo>, cache: Arc<dyn AuthCache>) -> Self {
        assert!(!config.access_secret.is_empty(), "JWT_SECRET is required");
        assert!(
            !config.refresh_secret.is_empty(),
            "JWT_REFRESH_SECRET is required"
        );

        let access_enc = EncodingKey::from_secret(config.access_secret.as_bytes());
        let access_dec = DecodingKey::from_secret(config.access_secret.as_bytes());
        let refresh_enc = EncodingKey::from_secret(config.refresh_secret.as_bytes());
        let refresh_dec = DecodingKey::from_secret(config.refresh_secret.as_bytes());

        Self {
            config: Arc::new(config),
            access_enc,
            access_dec,
            refresh_enc,
            refresh_dec,
            repo,
            cache,
        }
    }

    /// Sign a new access token for `user`.
    pub fn sign_access(
        &self,
        user: &User,
        providers: Vec<AuthProvider>,
    ) -> Result<String, AuthError> {
        let now = now_unix();
        let claims = AccessClaims {
            sub: user.id,
            jti: uuid::Uuid::new_v4(),
            email: user.email.clone(),
            full_name: user.full_name.clone(),
            verified: user.verified,
            providers,
            role: user.role.clone(),
            exp: now + self.config.access_ttl_secs,
            iat: now,
        };
        jsonwebtoken::encode(&jsonwebtoken::Header::default(), &claims, &self.access_enc)
            .map_err(|_| AuthError::TokenCreationFailed)
    }

    /// Decode and validate an access token.
    pub fn decode_access(&self, token: &str) -> Result<AccessClaims, AuthError> {
        let mut v = jsonwebtoken::Validation::default();
        v.validate_exp = true;
        v.leeway = 0;
        jsonwebtoken::decode::<AccessClaims>(token, &self.access_dec, &v)
            .map(|t| t.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::AccessTokenExpired,
                _ => AuthError::InvalidToken,
            })
    }

    pub fn decode_refresh(&self, token: &str) -> Result<RefreshClaims, AuthError> {
        let mut v = jsonwebtoken::Validation::default();
        v.validate_exp = true;
        v.leeway = 0;
        jsonwebtoken::decode::<RefreshClaims>(token, &self.refresh_dec, &v)
            .map(|t| t.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::RefreshTokenExpired,
                _ => AuthError::InvalidToken,
            })
    }

    /// Issue a fresh access + refresh pair for `user`, persist the refresh
    /// token, and return both tokens with metadata.
    pub async fn issue_pair(
        &self,
        user: &User,
        providers: Vec<AuthProvider>,
    ) -> Result<IssuedTokenPair, AuthError> {
        let access_token = self.sign_access(user, providers)?;

        let jti = uuid::Uuid::new_v4();
        let now = time::OffsetDateTime::now_utc();
        let expires_at = now + time::Duration::seconds(self.config.refresh_ttl_secs);

        let now_unix = now_unix();
        let refresh_claims = RefreshClaims {
            sub: user.id,
            jti,
            exp: now_unix + self.config.refresh_ttl_secs,
            iat: now_unix,
        };
        let refresh_token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &refresh_claims,
            &self.refresh_enc,
        )
        .map_err(|_| AuthError::TokenCreationFailed)?;

        self.repo
            .store_refresh_token(jti, user.id, &hash_token(&refresh_token), expires_at)
            .await?;

        Ok(IssuedTokenPair {
            access_token,
            refresh_token,
            refresh_expires_at: expires_at,
            refresh_jti: jti,
        })
    }

    /// Verify a refresh token, rotate it (delete old, insert new), and return
    /// a new pair. All DB operations run inside a transaction.
    pub async fn rotate(
        &self,
        refresh_token: &str,
        providers: Vec<AuthProvider>,
    ) -> Result<(IssuedTokenPair, User), AuthError> {
        let claims = self.decode_refresh(refresh_token)?;

        // Verify the token hash matches what's in the DB
        let stored = self
            .repo
            .find_refresh_token(claims.jti, claims.sub)
            .await?
            .ok_or(AuthError::RefreshTokenNotFound)?;

        if !verify_token_hash(refresh_token, &stored.token_hash) {
            return Err(AuthError::InvalidToken);
        }

        // Double-check DB-level expiry (defence in depth — JWT expiry already checked)
        if stored.expires_at < time::OffsetDateTime::now_utc() {
            self.repo.revoke_refresh_token(claims.jti).await?;
            return Err(AuthError::RefreshTokenExpired);
        }

        let user = self
            .repo
            .find_by_id(claims.sub)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        let new_jti = uuid::Uuid::new_v4();
        let now = time::OffsetDateTime::now_utc();
        let new_expires_at = now + time::Duration::seconds(self.config.refresh_ttl_secs);

        let now_unix = now_unix();
        let new_refresh_claims = RefreshClaims {
            sub: user.id,
            jti: new_jti,
            exp: now_unix + self.config.refresh_ttl_secs,
            iat: now_unix,
        };
        let new_refresh_token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &new_refresh_claims,
            &self.refresh_enc,
        )
        .map_err(|_| AuthError::TokenCreationFailed)?;

        self.repo
            .rotate_refresh_token(
                user.id,
                claims.jti,
                new_jti,
                &hash_token(&new_refresh_token),
                new_expires_at,
            )
            .await?;

        let access_token = self.sign_access(&user, providers)?;

        Ok((
            IssuedTokenPair {
                access_token,
                refresh_token: new_refresh_token,
                refresh_expires_at: new_expires_at,
                refresh_jti: new_jti,
            },
            user,
        ))
    }

    /// Revoke a single refresh token and optionally block the access token.
    pub async fn revoke(
        &self,
        refresh_token: &str,
        access_token: Option<&str>,
    ) -> Result<(), AuthError> {
        let claims = self.decode_refresh(refresh_token)?;
        self.repo.revoke_refresh_token(claims.jti).await?;

        if let Some(at) = access_token {
            // Best-effort: if the access token is expired/invalid, no need to block it
            if let Ok(ac) = self.decode_access(at) {
                let now = now_unix();
                let remaining = ac.exp.saturating_sub(now);
                if remaining > 0 {
                    self.cache.block_access_token(ac.jti, remaining).await?;
                }
            }
        }
        Ok(())
    }

    /// Revoke ALL refresh tokens for a user (e.g. password reset).
    pub async fn revoke_all_for_user(&self, user_id: uuid::Uuid) -> Result<(), AuthError> {
        self.repo.revoke_all_refresh_tokens(user_id).await?;
        // Block any outstanding access tokens for the access TTL window
        self.cache
            .block_all_user_tokens(user_id, self.config.access_ttl_secs)
            .await?;
        Ok(())
    }

    /// Check if an access token's JTI is on the blocklist.
    pub async fn is_access_token_blocked(
        &self,
        jti: uuid::Uuid,
        user_id: uuid::Uuid,
        issued_at: i64,
    ) -> Result<bool, AuthError> {
        let (jti_blocked, user_blocked) = tokio::try_join!(
            self.cache.is_token_blocked(jti),
            self.cache.is_user_tokens_blocked(user_id, issued_at),
        )?;
        Ok(jti_blocked || user_blocked)
    }

    #[must_use]
    pub fn access_ttl_secs(&self) -> i64 {
        self.config.access_ttl_secs
    }
}

fn now_unix() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

#[must_use]
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[must_use]
pub fn verify_token_hash(token: &str, stored_hash: &str) -> bool {
    hash_token(token) == stored_hash
}
