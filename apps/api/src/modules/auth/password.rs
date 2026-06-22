use anyhow::Result;
use argon2::password_hash::Error;
use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use std::sync::LazyLock;

// Tune upward until hash_password takes ~500ms on your production hardware
fn argon2() -> Argon2<'static> {
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(19456, 2, 1, None).expect("valid argon2 params"),
    )
}

pub static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    hash_password("not-a-real-password-used-only-for-timing-safety").expect("dummy hash")
});

pub fn hash_password(password: &str) -> Result<String, Error> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = argon2().hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

#[must_use]
pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .ok()
        .is_some_and(|h| argon2().verify_password(password.as_bytes(), &h).is_ok())
}

#[must_use]
pub async fn verify_password_async(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || verify_password(&password, &hash))
        .await
        .unwrap_or(false)
}
