use crate::modules::auth::errors::AuthError;
use crate::modules::auth::model::{AccountProvider, UserRole};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

/// Owns the encoding/decoding keys, so secrets are only parsed once at startup,
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
        let mut validation = Validation::default();
        validation.validate_exp = true;
        validation.leeway = 0;

        decode::<AccessClaims>(token, &self.access_decoding_key, &validation)
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
        let mut validation = Validation::default();
        validation.validate_exp = true;
        validation.leeway = 0;

        decode::<RefreshClaims>(token, &self.refresh_decoding_key, &validation)
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

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub fn verify_token_hash(token: &str, stored_hash: &str) -> bool {
    let computed_hash = hash_token(token);
    computed_hash == stored_hash
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_USER_ID: &str = "cabdb691-0ea8-4edf-928c-a9926418836a";
    const TEST_FULL_NAME: &str = "John Doe";
    const TEST_EMAIL: &str = "johndoe@example.com";

    fn setup() -> JwtService {
        JwtService::new(
            "3c627b443d66b86547acf70c6aa3f9277e7abe85417b9260d34b7a51d91b5ddedbb3dfdcb713f4bdb4f6581e2a73499217bca9e9b0a21d2c2f102dd2b581d5ff",
            "36a45f0c13f17237ee3e676021b0f480f1c50a47089f91edd3a447949f4c24d87013bf26d39342ebd0b151ef79172dfa9bf4abbe060772823ba6563b1a92ee06",
            900,
            604800,
        )
    }

    #[test]
    fn test_sign_and_decode_access_token() {
        let svc = setup();
        let user_id = Uuid::parse_str(TEST_USER_ID).expect("valid UUID");

        let token = svc
            .sign_access(
                user_id,
                &TEST_EMAIL,
                &TEST_FULL_NAME,
                true,
                AccountProvider::Email,
                UserRole::User,
            )
            .expect("sign_access should succeed");

        let claims = svc
            .decode_access(&token)
            .expect("decode_access should succeed");

        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.email, TEST_EMAIL);
        assert_eq!(claims.full_name, TEST_FULL_NAME);
        assert!(claims.verified);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_sign_and_decode_refresh_token() {
        let svc = setup();
        let user_id = Uuid::new_v4();

        let (token, jti) = svc
            .sign_refresh(user_id)
            .expect("sign_refresh should succeed");

        let claims = svc
            .decode_refresh(&token)
            .expect("decode_refresh should succeed");

        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.jti, jti);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_expired_access_token_returns_correct_error() {
        let svc = JwtService::new(
            "3c627b443d66b86547acf70c6aa3f9277e7abe85417b9260d34b7a51d91b5ddedbb3dfdcb713f4bdb4f6581e2a73499217bca9e9b0a21d2c2f102dd2b581d5ff",
            "36a45f0c13f17237ee3e676021b0f480f1c50a47089f91edd3a447949f4c24d87013bf26d39342ebd0b151ef79172dfa9bf4abbe060772823ba6563b1a92ee06",
            2,
            604800,
        );

        let user_id = Uuid::parse_str(TEST_USER_ID).expect("valid UUID");

        let token = svc
            .sign_access(
                user_id,
                &TEST_EMAIL,
                &TEST_FULL_NAME,
                true,
                AccountProvider::Email,
                UserRole::User,
            )
            .expect("sign should succeed");

        // Small sleep to ensure token is expired
        std::thread::sleep(std::time::Duration::from_secs(3));

        let err = svc.decode_access(&token).unwrap_err();
        assert!(matches!(err, AuthError::AccessTokenExpired));
    }

    #[test]
    fn test_invalid_token_returns_correct_error() {
        let svc = setup();
        let err = svc.decode_access("not.a.valid.token").unwrap_err();
        assert!(matches!(err, AuthError::InvalidToken));
    }

    #[test]
    fn test_hash_token_is_deterministic() {
        let hash1 = hash_token("some_refresh_token");
        let hash2 = hash_token("some_refresh_token");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_token_is_unique_per_input() {
        let hash1 = hash_token("token_a");
        let hash2 = hash_token("token_b");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_verify_token_hash() {
        let token = "some_refresh_token";
        let hash = hash_token(token);
        assert!(verify_token_hash(token, &hash));
        assert!(!verify_token_hash("wrong_token", &hash));
    }
}
