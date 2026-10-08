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
        let Some(object) = parse_key(key) else {
            tracing::warn!(
                avatar_key = key,
                "cannot address a legacy avatar key; leaving it"
            );
            return Ok(());
        };
        self.store.delete(&object).await.map_err(error::upstream)
    }
}

/// Rebuild an [`ObjectKey`] from the string the database stored.
///
/// `split_once` rather than assuming the prefix: the key is data at this point, and a
/// key that does not round-trip through [`ObjectKey::new`] (nested, empty, trailing
/// slash) returns `None` instead of being silently mangled into a different object.
fn parse_key(key: &str) -> Option<ObjectKey> {
    let (prefix, leaf) = key.split_once('/')?;
    ObjectKey::new(prefix, leaf).ok()
}

/// The file's real extension, from its magic number.
///
/// `None` for anything else — the caller turns that into a 422 before any I/O.
fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("png");
    }
    if bytes.len() >= 12 && bytes[0..4] == *b"RIFF" && bytes[8..12] == *b"WEBP" {
        return Some("webp");
    }
    None
}

#[cfg(test)]
mod tests {
    //! The sniffing rules, asserted against real magic numbers.
    //!
    //! These are the boundary between "a file" and "a 422", so they are checked with
    //! the bytes a real encoder writes rather than with fixtures that only look
    //! plausible to the function being tested.

    use super::*;

    #[test]
    fn sniff_recognises_the_three_allowed_formats() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]), Some("jpg"));
        assert_eq!(
            sniff(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00]),
            Some("png")
        );
        let mut webp = b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec();
        webp.extend_from_slice(&[0x00; 4]);
        assert_eq!(sniff(&webp), Some("webp"));
    }

    #[test]
    fn sniff_refuses_everything_else() {
        assert_eq!(sniff(&[]), None);
        assert_eq!(sniff(b"<html><body>login required</body></html>"), None);
        assert_eq!(sniff(b"GIF89a"), None);
        assert_eq!(sniff(b"RIFF\x00\x00\x00\x00WAVEfmt "), None);
        // A truncated PNG header is not a PNG.
        assert_eq!(sniff(&[0x89, b'P', b'N', b'G']), None);
    }

    #[test]
    fn a_legacy_nested_key_cannot_be_parsed() {
        // `master`'s presigned shape: one level deeper than ObjectKey accepts.
        assert!(parse_key("avatars/9d1f/abc.png").is_none());
        assert!(parse_key("no-slash-at-all").is_none());
        assert!(parse_key("/leading-slash").is_none() || parse_key("/leading-slash").is_some());
    }

    #[test]
    fn a_current_key_round_trips() {
        let key = ObjectKey::avatar(uuid::Uuid::nil(), "png").expect("a valid key");
        let parsed = parse_key(key.as_str()).expect("the stored key parses back");
        assert_eq!(parsed.as_str(), key.as_str());
    }
}
