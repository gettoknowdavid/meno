//! The in-memory [`ObjectStore`] — §5.6's "in-memory test double".
//!
//! Its own file because it is used by every consumer of this module (handlers, module
//! tests, and any future integration test) and because [`InMemoryObjectStore`] plus its
//! tests are a self-contained unit: someone looking for "how do I fake storage" should
//! find it without reading the adapter first (§9.3).

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;
use bytes::Bytes;

use crate::infrastructure::storage::error::StorageError;
use crate::infrastructure::storage::store::{ObjectKey, ObjectStore};

/// Backs the three operations with a `Mutex<HashMap>`, which is enough to assert the
/// real behaviours: that a second `put` replaces rather than duplicates, that `exists`
/// tracks it, and that `delete` is idempotent.
///
/// A `Mutex` rather than a lock-free map because the point is fidelity to the store's
/// *semantics*, not its concurrency, and a plain map keeps the double short enough to read
/// in one sitting. `lock()`'s poison handling is explicit rather than `.expect`-ed because
/// §9.1 forbids panicking in request-serving code.
pub struct InMemoryObjectStore {
    objects: Mutex<HashMap<String, Bytes>>,
    public_url: String,

    /// Set by [`Self::fail_next_put`]; taken by the next `put`.
    failures: Mutex<Option<StorageError>>,
}

impl InMemoryObjectStore {
    /// An empty store with a public base URL, e.g. `https://cdn.test`.
    #[must_use]
    pub fn new(public_url: impl Into<String>) -> Self {
        Self {
            objects: Mutex::new(HashMap::new()),
            public_url: public_url.into().trim_end_matches('/').to_owned(),
            failures: Mutex::new(None),
        }
    }

    /// The bytes stored at `key`, if any.
    ///
    /// Reads back what was written. Not on the trait — the real adapter has no such
    /// method, because getting an object out is a CDN fetch (§3.3) — but a test
    /// asserting that an upload stored the *right bytes* needs it.
    #[must_use]
    pub fn contents(&self, key: &ObjectKey) -> Option<Bytes> {
        self.objects
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key.as_str())
            .cloned()
    }

    /// How many objects are stored.
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Whether the store holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Make the next `put` fail, to exercise the error path.
    ///
    /// Returns the previous setting. There is no way to make a real S3 store fail on
    /// demand, so a failure-injection hook is what keeps the caller's error handling
    /// under test rather than only its happy path.
    pub fn fail_next_put(&self, error: Option<StorageError>) -> Option<StorageError> {
        let mut slot = self.failures.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::replace(&mut *slot, error)
    }
}

#[async_trait]
impl ObjectStore for InMemoryObjectStore {
    async fn put(&self, key: &ObjectKey, bytes: Bytes) -> Result<(), StorageError> {
        if let Some(error) = self
            .failures
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            return Err(error);
        }

        self.objects
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.as_str().to_owned(), bytes);

        Ok(())
    }

    async fn exists(&self, key: &ObjectKey) -> Result<bool, StorageError> {
        Ok(self
            .objects
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains_key(key.as_str()))
    }

    async fn delete(&self, key: &ObjectKey) -> Result<(), StorageError> {
        self.objects
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(key.as_str());

        Ok(())
    }

    fn public_url(&self, key: &ObjectKey) -> Option<String> {
        if self.public_url.is_empty() {
            return None;
        }
        Some(format!("{}/{}", self.public_url, key.as_str()))
    }
}

impl fmt::Debug for InMemoryObjectStore {
    /// Never prints object contents.
    ///
    /// A `{:?}` in an assertion failure on a test that uploaded an avatar would otherwise
    /// dump the bytes into the test output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryObjectStore")
            .field("objects", &self.len())
            .field("public_url", &self.public_url)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the double itself: the behaviours every caller of [`ObjectStore`]
    //! depends on, asserted once here rather than once per implementation.

    use uuid::Uuid;

    use super::*;

    fn key() -> ObjectKey {
        ObjectKey::avatar(Uuid::from_u128(1), "png").expect("a valid avatar key")
    }

    fn store() -> InMemoryObjectStore {
        InMemoryObjectStore::new("https://cdn.test")
    }

    // ── the double itself ──────────────────────────────────────────────────

    #[tokio::test]
    async fn a_put_stores_the_bytes_that_were_given() {
        let store = store();
        let payload = Bytes::from_static(b"png-bytes");

        store.put(&key(), payload.clone()).await.expect("a put");

        assert_eq!(store.contents(&key()), Some(payload));
    }

    #[tokio::test]
    async fn a_second_put_replaces_rather_than_duplicating() {
        // The avatar-replacement path: one key per user means this is an overwrite, and
        // the old object must not linger.
        let store = store();
        store
            .put(&key(), Bytes::from_static(b"old"))
            .await
            .expect("a put");

        store
            .put(&key(), Bytes::from_static(b"new"))
            .await
            .expect("a put");

        assert_eq!(store.len(), 1);
        assert_eq!(store.contents(&key()), Some(Bytes::from_static(b"new")));
    }

    #[tokio::test]
    async fn exists_tracks_what_was_written() {
        let store = store();
        assert!(!store.exists(&key()).await.expect("an exists"));

        store
            .put(&key(), Bytes::from_static(b"x"))
            .await
            .expect("a put");
        assert!(store.exists(&key()).await.expect("an exists"));

        store.delete(&key()).await.expect("a delete");
        assert!(!store.exists(&key()).await.expect("an exists"));
    }

    #[tokio::test]
    async fn deleting_an_absent_key_succeeds() {
        // Idempotent by contract: the caller is cleaning up after a replacement upload
        // and must not care whether the old avatar was already gone.
        store().delete(&key()).await.expect("a delete");
    }

    #[tokio::test]
    async fn a_failing_put_surfaces_the_injected_error() {
        // Otherwise the caller's error handling is never exercised — a real S3 store
        // cannot be told to fail on demand.
        let store = store();
        store.fail_next_put(Some(StorageError::Upstream("503".to_owned())));

        let error = store
            .put(&key(), Bytes::from_static(b"x"))
            .await
            .expect_err("must fail");

        assert!(error.is_retryable());
        assert!(store.is_empty(), "a failed put must not store anything");
    }

    #[tokio::test]
    async fn an_injected_failure_applies_only_once() {
        let store = store();
        store.fail_next_put(Some(StorageError::Upstream("503".to_owned())));

        let _ = store.put(&key(), Bytes::from_static(b"x")).await;
        store
            .put(&key(), Bytes::from_static(b"x"))
            .await
            .expect("the second put must succeed");
    }

    #[tokio::test]
    async fn the_double_never_prints_object_contents() {
        let store = store();
        store
            .put(&key(), Bytes::from_static(b"a-private-recording"))
            .await
            .expect("a put");

        let rendered = format!("{store:?}");
        assert!(
            !rendered.contains("a-private-recording"),
            "Debug must not dump object bytes: {rendered}"
        );
    }
}
