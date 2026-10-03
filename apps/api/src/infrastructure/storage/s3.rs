//! The S3-compatible [`ObjectStore`]: Cloudflare R2 in production, MinIO in dev.
//!
//! # Why one type and not `R2Store` + `MinioStore`
//!
//! §3.3 names two implementations, but R2 and MinIO differ in exactly four builder
//! options — region (`auto` vs `us-east-1`), whether plain HTTP is allowed, path-style vs
//! virtual-hosted addressing, and the default endpoint. Two types would be two copies of
//! every `put`/`exists`/`delete` body differing only in a `match` on those four settings,
//! and §9.3's file-size rule plus the usual drift argument both say don't. So the shape is
//! one [`S3ObjectStore`] plus a [`S3Flavor`] that carries the differences.
//!
//! The *behavioural* difference the plan actually cares about — R2 being read through a
//! CDN with no signing, MinIO through its own endpoint — is [`Self::public_url`], which is
//! configuration, not a type.
//!
//! # Reads are unsigned
//!
//! §3.3's decision, implemented in [`S3ObjectStore::public_url`]: a bucket bound to a
//! custom domain serves public assets directly, so there is no `Signer` call anywhere in
//! this file. Uploads come *in* through [`ObjectStore::put`] (the API proxies them); reads
//! never come back through this adapter at all, because they go CDN → client.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use object_store::aws::{AmazonS3, AmazonS3Builder};
use object_store::path::Path;
use object_store::{ObjectStoreExt, PutPayload};

use crate::config::StorageSettings;
use crate::infrastructure::storage::error::StorageError;
use crate::infrastructure::storage::store::{ObjectKey, ObjectStore};

/// Which S3-compatible service is on the other end.
///
/// Only the settings that actually differ. Everything else — credentials, bucket name,
/// public URL — comes from [`StorageSettings`] either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum S3Flavor {
    /// Cloudflare R2. Production.
    CloudflareR2,
    /// MinIO. The local dev/docker container.
    Minio,
}

impl S3Flavor {
    /// The region R2 requires.
    ///
    /// R2 rejects any other value, including the `us-east-1` that is correct for S3 and
    /// MinIO. It is a constant rather than configuration because getting it wrong is an
    /// opaque 400 from the API, not a build error.
    const R2_REGION: &'static str = "auto";

    /// The region MinIO defaults to when `STORAGE_REGION` is unset.
    const MINIO_REGION: &'static str = "us-east-1";

    /// Build the client for this flavor.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Config`] when the endpoint is unparseable or the bucket
    /// name is missing. `master` called `.expect("Failed to build storage client")` here,
    /// which turned a typo in `STORAGE_ENDPOINT` into a panic trace at boot (§9.1).
    fn build(self, settings: &StorageSettings) -> Result<AmazonS3, StorageError> {
        // `allow_http` follows the endpoint's own scheme rather than a flag: an operator
        // who wrote `http://` for a dev container meant it, and one who wrote `https://`
        // for R2 gets TLS enforced. Deriving it means the two cannot disagree.
        // `object_store` accepts a bare `"not a url"` as an endpoint and only fails on
        // the first request, which would surface as a 503 on an upload rather than a
        // startup error. Validating the shape here is what makes the failure actionable.
        let endpoint = settings.endpoint.trim();
        if !endpoint.starts_with("https://") && !endpoint.starts_with("http://") {
            return Err(StorageError::Config(
                "STORAGE_ENDPOINT must start with http:// or https://".to_owned(),
            ));
        }
        if settings.bucket.trim().is_empty() {
            return Err(StorageError::Config(
                "STORAGE_BUCKET must not be empty".to_owned(),
            ));
        }

        let allow_http = endpoint.starts_with("http://");

        let builder = AmazonS3Builder::new()
            .with_endpoint(endpoint)
            .with_access_key_id(&settings.access_key)
            .with_secret_access_key(settings.secret_key.expose())
            .with_bucket_name(&settings.bucket)
            .with_region(match self {
                Self::CloudflareR2 => Self::R2_REGION,
                Self::Minio => {
                    if settings.region.trim().is_empty() {
                        Self::MINIO_REGION
                    } else {
                        settings.region.as_str()
                    }
                }
            })
            // Path-style on both. R2's S3 endpoint does not support virtual-hosted
            // addressing, and MinIO in compose is reached at a bare host:port. Setting it
            // false is what the old builder did and it was right for both.
            .with_virtual_hosted_style_request(false)
            .with_allow_http(allow_http);

        builder.build().map_err(|e| {
            StorageError::Config(format!(
                "STORAGE_ENDPOINT/BUCKET/REGION are not usable: {e}"
            ))
        })
    }
}

/// The production [`ObjectStore`].
pub struct S3ObjectStore {
    store: Arc<AmazonS3>,
    public_url: String,
    flavor: S3Flavor,
}

impl S3ObjectStore {
    /// Build a store from validated settings.
    ///
    /// Takes [`StorageSettings`], not `&Config`: the adapter depends on six strings, and
    /// `config` is an application concern. `master` took `&Config` and reached through it
    /// into six flat fields.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Config`] when the client cannot be built.
    pub fn new(flavor: S3Flavor, settings: &StorageSettings) -> Result<Arc<Self>, StorageError> {
        Ok(Arc::new(Self {
            store: Arc::new(flavor.build(settings)?),
            public_url: settings.public_url.trim_end_matches('/').to_owned(),
            flavor,
        }))
    }

    /// Which service this store points at.
    ///
    /// Only for the startup log line — the behaviour is identical, so nothing branches on
    /// it.
    #[must_use]
    pub const fn flavor(&self) -> S3Flavor {
        self.flavor
    }
}

impl std::fmt::Debug for S3ObjectStore {
    /// Hand-written because [`AmazonS3`]'s `Debug` includes the request signer, and the
    /// signer holds the secret access key (§9.5).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3ObjectStore")
            .field("flavor", &self.flavor)
            .field("public_url", &self.public_url)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ObjectStore for S3ObjectStore {
    async fn put(&self, key: &ObjectKey, bytes: Bytes) -> Result<(), StorageError> {
        let path = Path::from(key.as_str());
        // Overwrite unconditionally: every caller upserts. `PutPayload::from(Bytes)`
        // keeps the body a single chunk, which is right for avatars and images.
        self.store
            .put(&path, PutPayload::from(bytes))
            .await
            .map_err(StorageError::from)
            .map(|_| ())
    }

    async fn exists(&self, key: &ObjectKey) -> Result<bool, StorageError> {
        let path = Path::from(key.as_str());
        match self.store.head(&path).await {
            Ok(_) => Ok(true),
            // A missing object is an answer. Mapping it to `Err` would push a `head`
            // behind the same error path as an outage and make "no avatar yet" look like
            // a 503.
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(error) => Err(StorageError::from(error)),
        }
    }

    async fn delete(&self, key: &ObjectKey) -> Result<(), StorageError> {
        let path = Path::from(key.as_str());
        match self.store.delete(&path).await {
            Ok(_) => Ok(()),
            // Idempotent by design — see the trait docs.
            Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(error) => Err(StorageError::from(error)),
        }
    }

    fn public_url(&self, key: &ObjectKey) -> Option<String> {
        if self.public_url.is_empty() {
            return None;
        }
        Some(format!("{}/{}", self.public_url, key.as_str()))
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the parts of the S3 adapter that do not need a server.
    //!
    //! Building the client and joining the public URL are pure, so they are covered here.
    //! The three network calls are covered against the in-memory double in `mod.rs` —
    //! which is the point of the trait: the behaviour is tested once, not twice per
    //! implementation.

    use super::*;

    fn settings(endpoint: &str, public_url: &str) -> StorageSettings {
        StorageSettings {
            endpoint: endpoint.to_owned(),
            access_key: "test-key".to_owned(),
            secret_key: crate::config::Secret::new("test-secret"),
            bucket: "meno-test".to_owned(),
            region: String::new(),
            public_url: public_url.to_owned(),
        }
    }

    fn key() -> ObjectKey {
        ObjectKey::new("avatars", "1.png").expect("a valid key")
    }

    #[test]
    fn an_r2_client_builds_without_contacting_the_network() {
        // `build()` is pure — it parses the endpoint and assembles a request signer. If
        // this ever starts doing IO, the "misconfigured at startup" property is gone.
        let store = S3ObjectStore::new(
            S3Flavor::CloudflareR2,
            &settings(
                "https://acct.r2.cloudflarestorage.com",
                "https://cdn.meno.app",
            ),
        )
        .expect("an R2 client must build");

        assert_eq!(store.flavor(), S3Flavor::CloudflareR2);
    }

    #[test]
    fn a_minio_client_builds_over_plain_http() {
        let store = S3ObjectStore::new(
            S3Flavor::Minio,
            &settings(
                "http://localhost:9000",
                "http://localhost:9000/meno-uploads",
            ),
        )
        .expect("a MinIO client must build");

        assert_eq!(store.flavor(), S3Flavor::Minio);
    }

    #[test]
    fn an_unparseable_endpoint_is_a_config_error_not_a_panic() {
        // The §9.1 regression test for `master`'s
        // `.expect("Failed to build storage client")`.
        for endpoint in ["not a url at all", "s3.example.com", "   ", "ftp://x"] {
            let error =
                S3ObjectStore::new(S3Flavor::Minio, &settings(endpoint, "https://cdn.meno.app"))
                    .expect_err("an endpoint without a scheme must be rejected");

            assert!(
                matches!(error, StorageError::Config(_)),
                "expected a config error for {endpoint:?}, got {error:?}"
            );
        }
    }

    #[test]
    fn the_secret_key_is_never_printed() {
        // §9.5. `AmazonS3`'s own `Debug` carries the signer, so the manual impl is the
        // only thing standing between a `{:?}` in a log line and the secret access key.
        let store = S3ObjectStore::new(
            S3Flavor::CloudflareR2,
            &settings(
                "https://acct.r2.cloudflarestorage.com",
                "https://cdn.meno.app",
            ),
        )
        .expect("a client");

        let rendered = format!("{store:?}");
        assert!(
            !rendered.contains("test-secret"),
            "the secret access key must not appear in Debug output: {rendered}"
        );
    }

    #[test]
    fn the_public_url_is_unsigned_and_has_no_double_slash() {
        let store = S3ObjectStore::new(
            S3Flavor::CloudflareR2,
            // A trailing slash on the configured base URL is the easy way to produce
            // `https://cdn.meno.app//avatars/1.png`, which is a 404 on a real CDN.
            &settings(
                "https://acct.r2.cloudflarestorage.com",
                "https://cdn.meno.app/",
            ),
        )
        .expect("a client");

        let url = store.public_url(&key()).expect("a public URL");

        assert_eq!(url, "https://cdn.meno.app/avatars/1.png");
        assert!(
            !url.contains("X-Amz-Signature") && !url.contains("Signature="),
            "§3.3 requires no pre-signing on public reads: {url}"
        );
    }

    #[test]
    fn no_public_base_url_yields_no_url_rather_than_a_broken_one() {
        let store = S3ObjectStore::new(S3Flavor::Minio, &settings("http://localhost:9000", ""))
            .expect("a client");

        assert_eq!(store.public_url(&key()), None);
    }
}
