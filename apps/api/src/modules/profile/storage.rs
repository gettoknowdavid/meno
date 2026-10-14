//! Avatar storage: sniff the bytes, store the object, keep the row in step (§3.3).
//!
//! # Why sniffing
//!
//! `Content-Type` and the filename are the client's word for what the file is; the
//! first bytes are the file's own. Checking magic numbers before a single byte is
//! written means an `.exe` renamed to `.png` never becomes an object, and — more
//! practically — a client bug that posts an HTML error page cannot be stored as an
//! avatar and then served from the CDN as one.
//!
//! The extension comes from the sniff too, so two uploads of the same photo share a
//! key (`avatars/{user-id}.png`) and replacing an avatar overwrites its own object
//! rather than accumulating orphans. That is also what lets `service` compare the old
//! key with the new one as strings and skip a pointless delete.
//!
//! # Why the API proxies the upload
//!
//! `master` handed the client a pre-signed PUT URL and trusted it to come back and
//! PATCH the key in. §3.3 retires pre-signing: here the bytes pass through the API,
//! which is where a size limit and a type check can actually be enforced — a signed
//! URL delegates both to the client's honesty for the next ten minutes.

use std::sync::Arc;

use bytes::Bytes;
use uuid::Uuid;

use super::error;
use crate::infrastructure::storage::{ObjectKey, ObjectStore};
// The magic-number rules are shared with `broadcast::storage` — one definition of what
// an image is, so the avatar path and the cover-image path cannot drift apart.
use crate::infrastructure::storage::sniff_image_extension as sniff;
use meno_core::Error as MenoError;

/// The largest avatar the API will accept, in bytes.
///
/// 5 MB is comfortably above any phone camera's thumbnail-sized profile shot and
/// small enough that one upload cannot occupy a worker's memory for long — the body
/// limit on the route is this plus a megabyte of multipart framing.
pub const MAX_AVATAR_BYTES: usize = 5 * 1024 * 1024;

/// The profile module's storage handle: a store and nothing else.
///
/// Built in `state.rs` (§5.6) so this type never knows whether the store behind it is
/// S3, the in-memory double, or `NoopObjectStore` refusing every write.
#[derive(Clone)]
pub struct ProfileStorage {
    store: Arc<dyn ObjectStore>,
}

/// A stored avatar: the key the row should carry, and the URL to display.
#[derive(Debug, Clone)]
pub struct StoredAvatar {
    /// `avatars/{user-id}.{ext}` — what goes in `users.avatar_id`.
    pub key: String,
    /// The store's public URL for it, or `None` when no public base URL is configured.
    pub url: Option<String>,
}

impl ProfileStorage {
    /// Wrap the application's object store.
    #[must_use]
    pub const fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self { store }
    }

    /// Validate and store `bytes` as `user_id`'s avatar.
    ///
    /// # Errors
    ///
    /// - [`super::error::avatar_too_large`] when the body exceeds [`MAX_AVATAR_BYTES`]
    ///   (422, field `file`) — checked before the sniff because it is a length compare.
    /// - [`super::error::avatar_not_an_image`] when the bytes are not JPEG, PNG or
    ///   WebP (422, field `file`).
    /// - [`super::error::upstream`] when the store refuses or fails the write (503) —
    ///   including `STORAGE_ENABLED=false`, whose `NoopObjectStore` fails closed for
    ///   every write, so a disabled deployment answers 503 rather than pretending.
    pub async fn put_avatar(&self, user_id: Uuid, bytes: &[u8]) -> Result<StoredAvatar, MenoError> {
        if bytes.len() > MAX_AVATAR_BYTES {
            return Err(error::avatar_too_large(MAX_AVATAR_BYTES));
        }
        let extension = sniff(bytes).ok_or_else(error::avatar_not_an_image)?;

        let key =
            ObjectKey::avatar(user_id, extension).map_err(|e| error::internal("profile_key", e))?;
        let url = self.store.public_url(&key);

        self.store
            .put(&key, Bytes::copy_from_slice(bytes))
            .await
            .map_err(error::upstream)?;

        Ok(StoredAvatar {
            key: key.as_str().to_owned(),
            url,
        })
    }

    /// Delete a replaced avatar's object.
    ///
    /// Never fails the request: the row already points at the *new* object, so a
    /// failed delete only costs a orphaned blob, and failing the upload now would be
    /// reporting a problem the caller cannot fix. A key this process cannot parse —
    /// `master`'s presigned keys were `avatars/{user}/{file}`, one level deeper than
    /// [`ObjectKey`] accepts — is logged and skipped; `master` never deleted the old
    /// object at all, so this is still strictly better than what it replaces.
    ///
    /// # Errors
    ///
    /// [`super::error::upstream`] when the store itself errors (503).
    pub async fn delete_avatar(&self, key: &str) -> Result<(), MenoError> {
        let Some(object) = ObjectKey::from_stored(key) else {
            tracing::warn!(
                avatar_key = key,
                "cannot address a legacy avatar key; leaving it"
            );
            return Ok(());
        };
        self.store.delete(&object).await.map_err(error::upstream)
    }
}

#[cfg(test)]
mod tests {
    //! What this module adds on top of the shared pieces: the *wiring* between a
    //! sniffed extension, a key, and the store.
    //!
    //! The sniffing rules themselves are asserted once, in
    //! [`crate::infrastructure::storage::image`], and key parsing once, in
    //! [`crate::infrastructure::storage::store`] — the point of extracting them was that
    //! this file should not be a second place they are tested.

    use super::*;
    use crate::infrastructure::storage::InMemoryObjectStore;

    fn store() -> (Arc<InMemoryObjectStore>, ProfileStorage) {
        let memory = Arc::new(InMemoryObjectStore::new("https://cdn.test"));
        let storage = ProfileStorage::new(Arc::clone(&memory) as Arc<dyn ObjectStore>);
        (memory, storage)
    }

    #[tokio::test]
    async fn a_put_avatar_keys_the_object_by_user_and_sniffed_extension() {
        let (memory, storage) = store();
        let user_id = uuid::Uuid::from_u128(7);
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00];

        let stored = storage
            .put_avatar(user_id, &png)
            .await
            .expect("a real PNG header is accepted");

        let key = ObjectKey::avatar(user_id, "png").expect("a valid key");
        assert_eq!(stored.key, key.as_str());
        assert_eq!(stored.url, Some(format!("https://cdn.test/{key}")));
        assert!(
            memory.contents(&key).is_some(),
            "the bytes reached the store"
        );
    }

    #[tokio::test]
    async fn a_put_avatar_refuses_bytes_that_are_not_an_image() {
        let (memory, storage) = store();

        let error = storage
            .put_avatar(uuid::Uuid::from_u128(7), b"<html>nope</html>")
            .await
            .expect_err("HTML is not an avatar");

        assert_eq!(error.code(), meno_core::ErrorCode::ValidationFailed);
        assert!(
            memory.is_empty(),
            "nothing is written before the sniff passes"
        );
    }

    #[tokio::test]
    async fn a_legacy_key_is_left_alone_rather_than_mangled_into_another_object() {
        // `master`'s presigned shape is one level deeper than `ObjectKey` accepts, so the
        // only safe answer is "skip it" — parsing it loosely would delete a real object.
        let (memory, storage) = store();
        let real = ObjectKey::avatar(uuid::Uuid::from_u128(7), "png").expect("a valid key");
        memory
            .put(&real, bytes::Bytes::from_static(b"x"))
            .await
            .expect("seeding the store");

        storage
            .delete_avatar("avatars/9d1f/abc.png")
            .await
            .expect("a legacy key is logged, not an error");

        assert!(memory.contents(&real).is_some(), "the live avatar survives");
    }

    #[tokio::test]
    async fn a_current_key_is_deleted() {
        let (memory, storage) = store();
        let key = ObjectKey::avatar(uuid::Uuid::from_u128(7), "png").expect("a valid key");
        memory
            .put(&key, bytes::Bytes::from_static(b"x"))
            .await
            .expect("seeding the store");

        storage
            .delete_avatar(key.as_str())
            .await
            .expect("deleting the replaced avatar");

        assert!(memory.contents(&key).is_none());
    }
}
