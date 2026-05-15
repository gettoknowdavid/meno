use crate::modules::auth::model::{AccountProvider, UserRole};
use jsonwebtoken::errors::Error;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: Uuid,
    pub jti: Uuid,
    pub full_name: String,
    pub email: String,
    pub verified: bool,
    pub account_provider: AccountProvider,
    pub role: UserRole,
    pub iat: u64,
    pub exp: u64,
}

pub fn create_access_token(
    user_id: Uuid,
    full_name: &str,
    email: &str,
    verified: bool,
    account_provider: AccountProvider,
    role: UserRole,
    secret: String,
    expires_in_secs: u64,
) -> Result<String, Error> {
    let now = time::UtcDateTime::now().unix_timestamp() as u64;
    let claims = Claims {
        sub: user_id,
        jti: Uuid::new_v4(),
        full_name: full_name.to_string(),
        email: email.to_string(),
        verified,
        account_provider,
        role,
        iat: now,
        exp: now + expires_in_secs,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

pub fn decode_access_token(token: &str, secret: &str) -> Result<Claims, Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}
