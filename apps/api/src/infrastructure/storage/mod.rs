//! Object storage: avatars, broadcast images, note attachments.
//!
//! Ported from `apps/api/src/infrastructure/storage.rs` on `master` (`903c3ba`) and
//! reworked for the monorepo's layering rules.
//!
//! # What changed, and why
//!
//! - **An [`ObjectStore`] trait instead of a concrete `StorageService`** (§5.2, §5.6).
//!   `master` named `object_store::aws::AmazonS3` in its own constructor, so every test
//!   needed a MinIO container. Now there is one real implementation ([`S3ObjectStore`]),
//!   one no-op for `STORAGE_ENABLED=false` ([`NoopObjectStore`]), and one in-memory
//!   double ([`InMemoryObjectStore`]) that the whole suite runs against.
//! - **No pre-signing anywhere** (§3.3). `master` handed the client a presigned PUT URL
//!   valid for ten minutes; this has no `Signer` call at all. Reads are served straight
//!   off the CDN custom domain and uploads are proxied through the API via
//!   [`ObjectStore::put`]. See `store` for why.
//! - **`.expect("Failed to build storage client")` is gone** (§9.1). A typo in
//!   `STORAGE_ENDPOINT` is now a [`StorageError::Config`] that `bootstrap` turns into a
//!   readable startup failure.
//! - **Keys are validated** (§9.5) by an [`ObjectKey`] newtype, so `..` cannot reach the
//!   driver.
//! - **`anyhow::Result` is gone** (§4.2) in favour of [`StorageError`], which erases
//!   `object_store::Error` at the boundary.
//!
//! # Module map
//!
//! - [`store`] — the [`ObjectStore`] trait and [`ObjectKey`].
//! - [`s3`] — [`S3ObjectStore`], parameterised by [`S3Flavor`].
//! - [`memory`] — [`InMemoryObjectStore`], the in-memory double.
//! - [`error`] — [`StorageError`].
//!
//! [`S3Flavor`]: s3::S3Flavor
//! [`S3ObjectStore`]: s3::S3ObjectStore
//!
//! # Testing
//!
//! Everything except the three S3 calls is pure and tested inline. The calls themselves
//! are tested once, against [`InMemoryObjectStore`] — which is the return on the trait:
//! the behaviour is asserted in one place rather than twice per implementation.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;

use crate::config::{Config, StorageSettings};
use crate::infrastructure::storage::s3::{S3Flavor, S3ObjectStore};

pub mod error;
pub mod memory;
pub mod s3;
pub mod store;

pub use error::StorageError;
pub use memory::InMemoryObjectStore;
pub use store::{ObjectKey, ObjectStore};

/// The no-op [`ObjectStore`] used when `STORAGE_ENABLED` is not true (§4.6).
///
/// A real implementation rather than a `cfg`-gated stub, for the same reason push has
/// one: a build with storage disabled must still compile the upload routes, which now
/// return [`StorageError::Disabled`] instead of failing to build.
///
/// Uploads through it are refused rather than silently dropped. A caller that uploads an
/// avatar and gets `Ok(())` back would store the key in the database and serve a 404 from
/// the CDN forever; refusing at the boundary makes the misconfiguration visible on the
/// first request rather than the first support ticket.
pub struct NoopObjectStore;

#[async_trait]
impl ObjectStore for NoopObjectStore {
    async fn put(&self, _key: &ObjectKey, _bytes: Bytes) -> Result<(), StorageError> {
        Err(StorageError::Disabled)
    }

    async fn exists(&self, _key: &ObjectKey) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn delete(&self, _key: &ObjectKey) -> Result<(), StorageError> {
        Ok(())
    }

    fn public_url(&self, _key: &ObjectKey) -> Option<String> {
        None
    }
}

/// The store to use, from validated configuration (§4.6).
///
/// Returns [`NoopObjectStore`] when `STORAGE_ENABLED` is unset, which cannot fail —
/// every route compiles and every call site is unchanged. When storage *is* enabled it
/// can fail, on an unusable endpoint, and that is deliberately fatal at startup for the
/// same reason push is: an enabled adapter that cannot build is worse than a refusal to
/// boot, because uploads would fail one request at a time.
///
/// # Errors
///
/// Returns [`StorageError::Config`] when storage is enabled but the settings do not build
/// a client.
pub fn store_from_config(config: &Config) -> Result<Arc<dyn ObjectStore>, StorageError> {
    match &config.storage {
        // `S3ObjectStore::new` already hands back an `Arc`, so this is an unsizing
        // coercion rather than a second allocation.
        Some(settings) => Ok(S3ObjectStore::new(flavor_for(settings), settings)?),
        None => {
            tracing::info!("object storage is disabled (STORAGE_ENABLED is not true)");
            Ok(Arc::new(NoopObjectStore))
        }
    }
}

/// Which service a set of settings describes.
///
/// R2 is identified by its endpoint host, MinIO by the fallback, because §3.3's
/// "MinIO in dev" is the local container and there is no `STORAGE_PROVIDER` variable to
/// rely on. Wrong guess costs a failed client build at startup, not a wrong result.
fn flavor_for(settings: &StorageSettings) -> S3Flavor {
    if settings.endpoint.contains("r2.cloudflarestorage.com") {
        S3Flavor::CloudflareR2
    } else {
        S3Flavor::Minio
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the module's seams: the no-op store, the in-memory double, and
    //! [`store_from_config`].
    //!
    //! These are the tests §10 asks for and `master` could not have had: avatar-replacement
    //! behaviour with no bucket, no network and no container. Each runs in microseconds.

    use uuid::Uuid;

    use super::*;

    fn key() -> ObjectKey {
        ObjectKey::avatar(Uuid::from_u128(1), "png").expect("a valid avatar key")
    }

    fn store() -> InMemoryObjectStore {
        InMemoryObjectStore::new("https://cdn.test")
    }

    // ── the no-op ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn the_no_op_refuses_uploads_but_answers_reads_honestly() {
        // Refusing is the point: a silent `Ok(())` would have the caller store the key in
        // the database and serve a CDN 404 forever.
        let store = NoopObjectStore;
        let key = key();

        assert!(matches!(
            store.put(&key, Bytes::from_static(b"x")).await,
            Err(StorageError::Disabled)
        ));
        assert!(!store.exists(&key).await.expect("an exists"));
        assert!(store.delete(&key).await.is_ok());
        assert_eq!(store.public_url(&key), None);
    }

    #[test]
    fn the_no_op_and_the_real_store_are_both_usable_as_one_trait_object() {
        // §5.3: substitutability is the point of the trait, so it is asserted directly.
        let stores: Vec<Arc<dyn ObjectStore>> = vec![Arc::new(NoopObjectStore), Arc::new(store())];

        assert_eq!(stores.len(), 2);
    }

    // ── the factory ────────────────────────────────────────────────────────

    /// Every required variable, no storage.
    ///
    /// Built through the public [`crate::config::MapSource`] rather than by constructing
    /// a `Config` literal, so the factory is tested against exactly what `Config::load`
    /// would hand it.
    fn base_source() -> crate::config::MapSource {
        crate::config::MapSource::new()
            .with("ENV", "dev")
            .with("PORT", "8080")
            .with("DATABASE_URL", "postgres://u:p@localhost:5432/meno")
            .with("REDIS_URL", "redis://localhost:6379")
            .with("JWT_SECRET", "a-real-secret-value")
            .with("JWT_REFRESH_SECRET", "another-real-secret-value")
            .with("CORS_ORIGINS", "https://app.example.com")
    }

    #[tokio::test]
    async fn a_disabled_storage_builds_the_no_op_and_cannot_fail() {
        // The §4.6 property: with the flag off, `store_from_config` always succeeds, so
        // a deployment with no bucket still boots.
        let config = Config::from_source(&base_source()).expect("valid config");
        assert!(config.storage.is_none());

        let store = store_from_config(&config).expect("a disabled store must build");

        assert!(matches!(
            store.put(&key(), Bytes::from_static(b"x")).await,
            Err(StorageError::Disabled)
        ));
    }

    #[tokio::test]
    async fn an_explicit_false_flag_disables_storage_even_with_the_variables_set() {
        // The flag is authoritative (§4.6): an operator who sets `STORAGE_ENABLED=false`
        // against a populated `.env` must get the no-op, not the real client.
        let config = Config::from_source(
            &base_source()
                .with("STORAGE_ENABLED", "false")
                .with("STORAGE_ENDPOINT", "http://localhost:9000")
                .with("STORAGE_ACCESS_KEY", "k")
                .with("STORAGE_SECRET_KEY", "s")
                .with("STORAGE_BUCKET", "meno-uploads")
                .with("STORAGE_REGION", "us-east-1")
                .with("STORAGE_PUBLIC_URL", "http://localhost:9000/meno-uploads"),
        )
        .expect("valid config");

        assert!(config.storage.is_none());
        assert!(
            store_from_config(&config)
                .expect("a disabled store must build")
                .put(&key(), Bytes::from_static(b"x"))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_well_configured_minio_builds_the_real_adapter() {
        // The enabled path, end to end from an environment. Builds without contacting
        // anything, which is the property that keeps `bootstrap` fast and testable.
        let config = Config::from_source(
            &base_source()
                .with("STORAGE_ENDPOINT", "http://localhost:9000")
                .with("STORAGE_ACCESS_KEY", "rustfsadmin")
                .with("STORAGE_SECRET_KEY", "rustfspassword")
                .with("STORAGE_BUCKET", "meno-uploads")
                .with("STORAGE_REGION", "us-east-1")
                .with("STORAGE_PUBLIC_URL", "http://localhost:9000/meno-uploads"),
        )
        .expect("valid config");

        let store = store_from_config(&config).expect("a configured store must build");

        let url = store
            .public_url(&key())
            .expect("a public URL from STORAGE_PUBLIC_URL");

        assert_eq!(
            url,
            format!("http://localhost:9000/meno-uploads/{}", key()),
            "the CDN base URL and the key, joined by exactly one slash"
        );
        assert!(
            !url.contains("X-Amz-Signature"),
            "§3.3: public reads are served unsigned from the CDN: {url}"
        );
    }

    #[tokio::test]
    async fn an_enabled_but_broken_storage_fails_startup_rather_than_going_silent() {
        // The mirror image of the push adapter's property. An enabled adapter that cannot
        // build is worse than a refusal to boot: uploads would fail one request at a
        // time, and the first symptom would be a user reporting their avatar is broken.
        let config = Config::from_source(
            &base_source()
                .with("STORAGE_ENDPOINT", "localhost:9000")
                .with("STORAGE_ACCESS_KEY", "k")
                .with("STORAGE_SECRET_KEY", "s")
                .with("STORAGE_BUCKET", "meno-uploads")
                .with("STORAGE_REGION", "us-east-1")
                .with("STORAGE_PUBLIC_URL", "https://cdn.meno.app"),
        )
        .expect("valid config");

        let Err(error) = store_from_config(&config) else {
            panic!("a broken STORAGE_ENDPOINT must not produce a usable store");
        };

        assert!(
            matches!(error, StorageError::Config(_)),
            "expected a config error, got {error:?}"
        );
    }

    #[test]
    fn the_r2_endpoint_selects_the_r2_flavor() {
        let r2 = StorageSettings {
            endpoint: "https://acct.r2.cloudflarestorage.com".to_owned(),
            access_key: "k".to_owned(),
            secret_key: crate::config::Secret::new("s"),
            bucket: "b".to_owned(),
            region: "auto".to_owned(),
            public_url: "https://cdn.meno.app".to_owned(),
        };
        let minio = StorageSettings {
            endpoint: "http://localhost:9000".to_owned(),
            ..r2.clone()
        };

        assert_eq!(flavor_for(&r2), S3Flavor::CloudflareR2);
        assert_eq!(flavor_for(&minio), S3Flavor::Minio);
    }
}
