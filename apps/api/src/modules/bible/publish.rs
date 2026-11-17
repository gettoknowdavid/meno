//! The publish step: upload one seed payload under its versioned, immutable key.
//!
//! # Why this lives in the library
//!
//! `src/bin/publish-bible.rs` is argument parsing and a file read. The part worth testing —
//! that the upload carries the right key, the right cache headers, and that a payload the
//! catalogue does not describe is *refused* — is here, where it runs against
//! [`InMemoryObjectStore`](crate::infrastructure::storage::InMemoryObjectStore) with no
//! bucket and no network.
//!
//! # The catalogue is checked before the upload
//!
//! [`publish`] compares the file's byte length and SHA-256 with the entry it was asked to
//! publish. A mismatch is a refusal, not a warning: the endpoint hands those two values to
//! every client as the thing to verify a download against, so uploading a file they do not
//! describe would make every download fail verification on the device, long after the
//! release that caused it. The failure message names both sets of values so the fix is one
//! edit to [`TRANSLATIONS`](super::catalogue::TRANSLATIONS).
//!
//! # Immutability is a promise about the key
//!
//! The payload is cached for a year as `immutable`, which is only honest because the key
//! contains the version: `bible/asv/1.json.gz` is written once. A corrected text is
//! version 2 and a new object — publishing over a version that devices already hold would
//! leave them seeded from bytes that no longer match the hash they stored.

use bytes::Bytes;
use sha2::{Digest, Sha256};

use crate::infrastructure::storage::{ObjectKey, ObjectMetadata, ObjectStore, StorageError};
use crate::modules::bible::catalogue::CatalogueEntry;

/// The `Content-Type` a published payload is served with.
///
/// `application/gzip`, not `application/json`: the object *is* gzip, and a CDN or browser
/// told otherwise would try to decode or decompress it a second time. The client gunzips
/// exactly once.
const PAYLOAD_CONTENT_TYPE: &str = "application/gzip";

/// How long a payload may be cached: one year, and `immutable`.
///
/// Honest rather than a cache-poisoning hazard because the key carries the version — a
/// corrected text is `v2`, a new key, never a rewrite of `v1`. Lives here rather than in
/// the storage layer because it is a promise about this payload's key, not about storage.
const PAYLOAD_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

/// What one publish did.
///
/// Returned rather than logged so the tool prints the URL an operator can paste into a
/// browser, and so a test can assert the key and the hash without reading the store back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Published {
    /// The object key the payload was written under, e.g. `bible/asv/1.json.gz`.
    pub key: String,
    /// The payload's byte length.
    pub bytes: u64,
    /// The payload's SHA-256, lowercase hex — the same string the catalogue carries.
    pub sha256: String,
    /// The URL the payload is served from, or `None` when the store has no public base.
    pub url: Option<String>,
}

/// A refused or failed publish.
#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    /// The file does not match the catalogue entry it was published as.
    #[error(
        "the payload does not match the catalogue entry for {code} v{version}: the file is \
         {actual_bytes} bytes with sha256 {actual_sha256}, but the catalogue says \
         {expected_bytes} bytes with sha256 {expected_sha256} — put the file's values in \
         `TRANSLATIONS` (modules/bible/catalogue.rs) and publish again"
    )]
    CatalogueMismatch {
        /// The code the entry names.
        code: &'static str,
        /// The version the entry names.
        version: u32,
        /// Bytes the catalogue claims.
        expected_bytes: u64,
        /// SHA-256 the catalogue claims.
        expected_sha256: &'static str,
        /// Bytes the file actually has.
        actual_bytes: u64,
        /// SHA-256 the file actually has.
        actual_sha256: String,
    },

    /// The store refused the upload.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

/// The lowercase hex SHA-256 of `bytes`.
///
/// One definition, shared by the publish check and the tests that compute an expected
/// digest, so the two cannot disagree about encoding.
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Uploads `payload` as `entry`, under `bible/<code>/<version>.json.gz`.
///
/// # Errors
///
/// [`PublishError::CatalogueMismatch`] when the payload's length or digest is not the one
/// the catalogue publishes — nothing is uploaded in that case. [`PublishError::Storage`]
/// when the object store refuses the write, or the entry cannot become a key.
pub async fn publish(
    store: &dyn ObjectStore,
    entry: &CatalogueEntry,
    payload: Bytes,
) -> Result<Published, PublishError> {
    let sha256 = digest(&payload);
    let bytes = payload.len() as u64;

    if bytes != entry.bytes || sha256 != entry.sha256 {
        return Err(PublishError::CatalogueMismatch {
            code: entry.code,
            version: entry.version,
            expected_bytes: entry.bytes,
            expected_sha256: entry.sha256,
            actual_bytes: bytes,
            actual_sha256: sha256,
        });
    }

    let key = ObjectKey::bible_payload(entry.code, entry.version)?;
    let metadata = ObjectMetadata {
        content_type: Some(PAYLOAD_CONTENT_TYPE.to_owned()),
        cache_control: Some(PAYLOAD_CACHE_CONTROL.to_owned()),
    };
    store.put_with_metadata(&key, payload, &metadata).await?;

    Ok(Published {
        key: key.as_str().to_owned(),
        bytes,
        sha256,
        url: store.public_url(&key),
    })
}

#[cfg(test)]
mod tests {
    //! The properties a release depends on: the key, the headers, and the refusal.

    use super::*;
    use crate::infrastructure::storage::InMemoryObjectStore;

    const PAYLOAD: &[u8] = b"\x1f\x8b\x08\x00a-seed-payload";

    /// The catalogue entry that describes [`PAYLOAD`], computed at test time.
    ///
    /// Leaked on purpose: the catalogue's fields are `&'static str` because the real table
    /// is a `const`, and a test entry has to have the same shape or it would not exercise
    /// the same code path.
    fn entry_for(payload: &[u8]) -> CatalogueEntry {
        let sha256: &'static str = Box::leak(digest(payload).into_boxed_str());
        CatalogueEntry {
            code: "tst",
            name: "Test Translation",
            version: 3,
            bytes: payload.len() as u64,
            sha256,
        }
    }

    fn store() -> InMemoryObjectStore {
        InMemoryObjectStore::new("https://cdn.test")
    }

    #[test]
    fn the_digest_is_sha256_lowercase_hex() {
        // A fixed vector: this is the one value in the publish path that is a pure
        // function, so it is pinned rather than computed twice.
        assert_eq!(
            digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[tokio::test]
    async fn a_publish_lands_on_the_versioned_key_with_immutable_headers() {
        let store = store();
        let entry = entry_for(PAYLOAD);

        let published = publish(&store, &entry, Bytes::from_static(PAYLOAD))
            .await
            .expect("a publish");

        assert_eq!(published.key, "bible/tst/3.json.gz");
        assert_eq!(
            published.url.as_deref(),
            Some("https://cdn.test/bible/tst/3.json.gz")
        );
        assert_eq!(published.sha256, entry.sha256);

        let key = ObjectKey::bible_payload("tst", 3).expect("a key");
        assert_eq!(store.contents(&key), Some(Bytes::from_static(PAYLOAD)));
        let metadata = store.metadata(&key).expect("metadata was stored");
        assert_eq!(
            metadata.content_type.as_deref(),
            Some("application/gzip"),
            "the CDN must serve gzip as gzip, or a client decodes it twice"
        );
        assert_eq!(
            metadata.cache_control.as_deref(),
            Some(PAYLOAD_CACHE_CONTROL)
        );
    }

    #[tokio::test]
    async fn a_payload_the_catalogue_does_not_describe_is_refused_and_not_uploaded() {
        // The drift guard. The endpoint publishes `bytes` and `sha256` as the values to
        // verify a download against, so a file they do not describe must not reach the
        // bucket at all.
        let store = store();
        let entry = entry_for(PAYLOAD);

        let error = publish(&store, &entry, Bytes::from_static(b"a-different-payload"))
            .await
            .expect_err("must refuse");

        assert!(matches!(error, PublishError::CatalogueMismatch { .. }));
        assert!(store.is_empty(), "a refused publish writes nothing");
        // Both sets of values are in the message, because the fix is to edit the table.
        let message = error.to_string();
        assert!(message.contains("tst v3"), "{message}");
        assert!(message.contains(&digest(PAYLOAD)), "{message}");
        assert!(
            message.contains(&digest(b"a-different-payload")),
            "{message}"
        );
    }

    #[tokio::test]
    async fn a_payload_of_the_right_length_but_the_wrong_bytes_is_refused_too() {
        // The length check alone would pass here; the digest is what actually catches a
        // payload that was regenerated with a different text but the same verse set.
        let store = store();
        let mut entry = entry_for(PAYLOAD);
        entry.sha256 = Box::leak("0".repeat(64).into_boxed_str());

        let error = publish(&store, &entry, Bytes::from_static(PAYLOAD))
            .await
            .expect_err("must refuse");

        assert!(matches!(error, PublishError::CatalogueMismatch { .. }));
        assert!(store.is_empty());
    }

    #[tokio::test]
    async fn a_store_failure_surfaces_rather_than_being_reported_as_published() {
        // A release script that printed "published" on a rejected upload would be worse
        // than one that failed: the catalogue would advertise an object that is not there.
        let store = store();
        store.fail_next_put(Some(StorageError::Upstream("503".to_owned())));
        let entry = entry_for(PAYLOAD);

        let error = publish(&store, &entry, Bytes::from_static(PAYLOAD))
            .await
            .expect_err("must fail");

        assert!(matches!(error, PublishError::Storage(_)));
        assert!(store.is_empty());
    }
}
