use crate::modules::auth::errors::AuthError;
use crate::modules::auth::model::{AccountProvider, UserRole};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Payload embedded in every access token.
///
/// Keep this lean but complete enough that the auth middleware never
/// needs a DB or Redis round-trip on the hot path.
///
/// `jti` gives you per-token log correlation — essential when you have
/// hundreds of concurrent broadcast listeners all hitting chat/join endpoints.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AccessClaims {
    pub sub: Uuid,
    pub jti: Uuid,
    pub email: String,
    pub full_name: String,
    pub verified: bool,
    pub account_provider: AccountProvider,
    pub role: UserRole,
    pub exp: u64,
    pub iat: u64,
}

/// Payload embedded in every refresh token.
///
/// Intentionally minimal — the refresh token only needs to identify
/// the user and the specific DB row so we can rotate or revoke it.
/// No mutable user state here; the next access token gets fresh claims.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RefreshClaims {
    pub sub: Uuid,
    pub jti: Uuid,
    pub exp: u64,
    pub iat: u64,
}

/// Owns the encoding/decoding keys so secrets are only parsed once at startup,
/// not on every request. Cheap to clone (keys are Arc-backed internally).
#[derive(Clone)]
pub struct JwtService {
    access_encoding_key: EncodingKey,
    access_decoding_key: DecodingKey,
    refresh_encoding_key: EncodingKey,
    refresh_decoding_key: DecodingKey,
    access_expires_in: u64,
    refresh_expires_in: u64,
}
impl JwtService {
    pub fn new(
        access_secret: &str,
        refresh_secret: &str,
        access_expires_in: u64,
        refresh_expires_in: u64,
    ) -> Self {
        assert!(!access_secret.is_empty(), "JWT_SECRET required");
        assert!(!refresh_secret.is_empty(), "JWT_REFRESH_SECRET required");

        Self {
            access_encoding_key: EncodingKey::from_secret(access_secret.as_bytes()),
            access_decoding_key: DecodingKey::from_secret(access_secret.as_bytes()),
            refresh_encoding_key: EncodingKey::from_secret(refresh_secret.as_bytes()),
            refresh_decoding_key: DecodingKey::from_secret(refresh_secret.as_bytes()),
            access_expires_in,
            refresh_expires_in,
        }
    }

    pub fn sign_access(
        &self,
        user_id: Uuid,
        email: &str,
        full_name: &str,
        verified: bool,
        account_provider: AccountProvider,
        role: UserRole,
    ) -> Result<String, AuthError> {
        let now = now_unix();
        let claims = AccessClaims {
            sub: user_id,
            jti: Uuid::new_v4(),
            email: email.to_string(),
            full_name: full_name.to_string(),
            verified,
            account_provider,
            role,
            exp: now + self.access_expires_in,
            iat: now,
        };
        encode(&Header::default(), &claims, &self.access_encoding_key)
            .map_err(|_| AuthError::TokenCreationFailed)
    }

    pub fn decode_access(&self, token: &str) -> Result<AccessClaims, AuthError> {
        decode::<AccessClaims>(token, &self.access_decoding_key, &Validation::default())
            .map(|c| c.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::AccessTokenExpired,
                _ => AuthError::InvalidToken,
            })
    }

    /// Returns `(signed_token, jti)`.
    /// The caller stores `jti` as the primary key in a `refresh_tokens` table
    /// so that revocation is a single indexed DELETE by UUID — no full-table scan.
    pub fn sign_refresh(&self, user_id: Uuid) -> Result<(String, Uuid), AuthError> {
        let now = now_unix();
        let jti = Uuid::new_v4();
        let claims = RefreshClaims {
            sub: user_id,
            jti,
            exp: now + self.refresh_expires_in,
            iat: now,
        };
        let token = encode(&Header::default(), &claims, &self.refresh_encoding_key)
            .map_err(|_| AuthError::TokenCreationFailed)?;
        Ok((token, jti))
    }

    pub fn decode_refresh(&self, token: &str) -> Result<RefreshClaims, AuthError> {
        decode::<RefreshClaims>(token, &self.refresh_decoding_key, &Validation::default())
            .map(|c| c.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::RefreshTokenExpired,
                _ => AuthError::InvalidToken,
            })
    }
}

// Helpers
fn now_unix() -> u64 {
    time::OffsetDateTime::now_utc().unix_timestamp() as u64
}
