//! Cover-image storage: sniff the bytes, store the object, keep the row in step (§3.3).
//!
//! # Why the API proxies the upload
//!
//! The same reason `profile::storage` does, and the same decision (§3.3): `master` handed
//! the client a pre-signed PUT URL and trusted it to come back and PATCH the key in. Here
//! the bytes pass through the API, which is where a size limit and a type check can
//! actually be enforced — a signed URL delegates both to the client's honesty for the next
//! ten minutes. That is also what makes "create a broadcast with its cover" work in one
//! round trip: the client sends the image and the API, not the client, writes the key.
//!
//! # Why sniffing
//!
//! `Content-Type` and the filename are the client's word for what the file is; the first
//! bytes are the file's own. The rules live in
//! [`crate::infrastructure::storage::image`] so this path and the avatar path cannot
//! disagree about what an image is.
//!
//! The extension comes from the sniff too, so two uploads of the same picture share a key
//! (`broadcasts/{id}.png`) and replacing a cover overwrites its own object rather than
//! accumulating orphans. That is also what lets the service compare the old key with the
//! new one as strings and skip a pointless delete.

use std::sync::Arc;

use bytes::Bytes;
use uuid::Uuid;

use super::error;
use crate::infrastructure::storage::{ObjectKey, ObjectStore, sniff_image_extension};
use meno_core::Error as MenoError;

/// The largest cover image the API will accept, in bytes.
///
/// 5 MB, matching avatars: comfortably above anything a phone camera produces for a
/// cover, and small enough that one upload cannot occupy a worker's memory for long. The
/// body limit on the route is this plus a megabyte of multipart framing.
pub const MAX_BROADCAST_IMAGE_BYTES: usize = 5 * 1024 * 1024;

/// The broadcast module's storage handle: a store and nothing else.
///
/// Built in `state.rs` (§5.6) so this type never knows whether the store behind it is S3,
/// the in-memory double, or `NoopObjectStore` refusing every write.
#[derive(Clone)]
pub struct BroadcastStorage {
    store: Arc<dyn ObjectStore>,
}

/// A stored cover: the key the row should carry, and the URL to display.
#[derive(Debug, Clone)]
pub struct StoredImage {
    /// `broadcasts/{broadcast-id}.{ext}` — what goes in `broadcasts.image_id`.
    pub key: String,
    /// The store's public URL for it, or `None` when no public base URL is configured.
    pub url: Option<String>,
}

impl BroadcastStorage {
    /// Wrap the application's object store.
    #[must_use]
    pub const fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self { store }
    }

    /// A storage handle over [`NoopObjectStore`] — what a deployment with
    /// `STORAGE_ENABLED` unset gets, and what a test that is not about images gets.
    ///
    /// Uploads through it answer 503 rather than pretending (§4.6), so a module assembled
    /// without a bucket still compiles and still refuses honestly.
    #[must_use]
    pub fn disabled() -> Self {
        Self::new(Arc::new(crate::infrastructure::storage::NoopObjectStore))
    }

    /// Validate and store `bytes` as `broadcast_id`'s cover.
    ///
    /// # Errors
    ///
    /// - [`super::error::image_too_large`] when the body exceeds
    ///   [`MAX_BROADCAST_IMAGE_BYTES`] (422, field `file`) — checked before the sniff
    ///   because it is a length compare.
    /// - [`super::error::image_not_an_image`] when the bytes are not JPEG, PNG or WebP
    ///   (422, field `file`).
    /// - [`super::error::storage_unavailable`] when the store refuses or fails the write
    ///   (503) — including `STORAGE_ENABLED=false`, whose `NoopObjectStore` fails closed
    ///   for every write, so a disabled deployment answers 503 rather than pretending.
    pub async fn put_image(
        &self,
        broadcast_id: Uuid,
        bytes: &[u8],
    ) -> Result<StoredImage, MenoError> {
        if bytes.len() > MAX_BROADCAST_IMAGE_BYTES {
            return Err(error::image_too_large(MAX_BROADCAST_IMAGE_BYTES));
        }
        let extension = sniff_image_extension(bytes).ok_or_else(error::image_not_an_image)?;

        let key = ObjectKey::broadcast_image(broadcast_id, extension)
            .map_err(|e| error::internal("broadcast_image_key", e))?;
        let url = self.store.public_url(&key);

        self.store
            .put(&key, Bytes::copy_from_slice(bytes))
            .await
            .map_err(error::storage_unavailable)?;

        Ok(StoredImage {
            key: key.as_str().to_owned(),
            url,
        })
    }

    /// Delete an object this module stored.
    ///
    /// Never fails the request it is part of, for the reason `profile::storage` gives: the
    /// row already points at the *new* object, so a failed delete only costs an orphaned
    /// blob, and failing now would be reporting a problem the caller cannot fix. A key
    /// this process cannot address is logged and skipped rather than guessed at, because a
    /// loose parse would delete a *different* object.
    ///
    /// # Errors
    ///
    /// [`super::error::storage_unavailable`] when the store itself errors (503).
    pub async fn delete_image(&self, key: &str) -> Result<(), MenoError> {
        let Some(object) = ObjectKey::from_stored(key) else {
            tracing::warn!(
                image_key = key,
                "cannot address this cover image key; leaving it"
            );
            return Ok(());
        };
        self.store
            .delete(&object)
            .await
            .map_err(error::storage_unavailable)
    }
}

impl std::fmt::Debug for BroadcastStorage {
    /// The store is an adapter that may hold credentials, so a `{:?}` names the field and
    /// stops — the same rule the rest of the module follows.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BroadcastStorage")
            .field("store", &"")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    //! What this module adds on top of the shared pieces: the size limit, the key, and
    //! what reaches the store.
    //!
    //! The sniffing rules are asserted once in [`crate::infrastructure::storage::image`];
    //! these tests are about the *wiring* around them.

    use super::*;
    use crate::infrastructure::storage::InMemoryObjectStore;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00];

    fn store() -> (Arc<InMemoryObjectStore>, BroadcastStorage) {
        let memory = Arc::new(InMemoryObjectStore::new("https://cdn.test"));
        let storage = BroadcastStorage::new(Arc::clone(&memory) as Arc<dyn ObjectStore>);
        (memory, storage)
    }

    #[tokio::test]
    async fn an_image_is_stored_under_the_broadcast_key_with_a_public_url() {
        let (memory, storage) = store();
        let id = Uuid::from_u128(11);

        let stored = storage.put_image(id, PNG).await.expect("a real PNG");

        let key = ObjectKey::broadcast_image(id, "png").expect("a valid key");
        assert_eq!(stored.key, key.as_str());
        assert_eq!(stored.url, Some(format!("https://cdn.test/{key}")));
        assert!(memory.contents(&key).is_some());
    }

    #[tokio::test]
    async fn a_second_upload_of_the_same_format_overwrites_rather_than_accumulating() {
        // The property that makes the old key compare as a string worth doing: one object
        // per broadcast, so replacing a cover does not leave the previous one behind.
        let (memory, storage) = store();
        let id = Uuid::from_u128(11);

        let first = storage.put_image(id, PNG).await.expect("the first");
        let second = storage.put_image(id, PNG).await.expect("the second");

        assert_eq!(first.key, second.key);
        assert_eq!(memory.len(), 1, "one broadcast, one cover object");
    }

    #[tokio::test]
    async fn bytes_that_are_not_an_image_never_reach_the_store() {
        let (memory, storage) = store();

        let error = storage
            .put_image(Uuid::from_u128(11), b"<html>not an image</html>")
            .await
            .expect_err("HTML is not a cover");

        assert_eq!(error.code(), meno_core::ErrorCode::ValidationFailed);
        assert!(memory.is_empty());
    }

    #[tokio::test]
    async fn an_oversized_image_is_refused_before_the_sniff() {
        let (memory, storage) = store();
        let mut oversized = PNG.to_vec();
        oversized.resize(MAX_BROADCAST_IMAGE_BYTES + 1, 0);

        let error = storage
            .put_image(Uuid::from_u128(11), &oversized)
            .await
            .expect_err("one byte over the limit is over the limit");

        assert_eq!(error.code(), meno_core::ErrorCode::ValidationFailed);
        assert!(memory.is_empty());
    }

    #[tokio::test]
    async fn a_disabled_store_refuses_the_write_instead_of_pretending() {
        // §4.6: the module compiles without a bucket, and an upload says so loudly rather
        // than storing a key that would 404 from the CDN forever.
        let storage = BroadcastStorage::disabled();

        let error = storage
            .put_image(Uuid::from_u128(11), PNG)
            .await
            .expect_err("no storage is configured");

        assert_eq!(error.code(), meno_core::ErrorCode::UpstreamUnavailable);
    }

    #[tokio::test]
    async fn an_unaddressable_key_is_left_alone() {
        let (memory, storage) = store();
        let real = ObjectKey::broadcast_image(Uuid::from_u128(11), "png").expect("a valid key");
        memory
            .put(&real, Bytes::from_static(b"x"))
            .await
            .expect("seeding the store");

        storage
            .delete_image("broadcasts/9d1f/abc.png")
            .await
            .expect("an unparseable key is logged, not an error");

        assert!(memory.contents(&real).is_some());
    }
}
