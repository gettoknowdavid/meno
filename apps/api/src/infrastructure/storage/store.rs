//! The [`ObjectStore`] seam: what the application asks of object storage.
//!
//! # Why a trait (§5.2, §5.6)
//!
//! `master`'s `StorageService` named `object_store::aws::AmazonS3` in its own signature,
//! so every caller that wanted to test "does an upload replace the old avatar" needed a
//! MinIO container. This is the §5.6 fix applied to the second adapter trait the plan
//! asks for: one trait, one real implementation ([`S3ObjectStore`]), and an in-memory
//! double, so the whole module is unit-testable with no network — which §10's unit tier
//! requires.
//!
//! # No pre-signing anywhere
//!
//! §3.3 records the decision: R2 buckets are private by default, and the plan's answer is
//! to bind a **custom domain** (`cdn.meno.app`) to the bucket so public assets are read
//! straight from the CDN with no signature. That is why there is no
//! `presigned_upload_url` on this trait and no `Signer` usage in the module: reads are
//! [`Self::public_url`] — a string join — and uploads go *through the API* via
//! [`Self::put`].
//!
//! The old presigned-PUT method was not merely a style difference. It forced a 10-minute
//! window during which anyone holding the URL could overwrite the object, and it meant
//! the mobile client could not show upload progress through the API. Streaming via
//! [`Self::put`] costs the API some egress (§3.3 accepts that for private assets, and
//! accepts it here by choice) and keeps authorisation in one place.
//!
//! # What is deliberately not here
//!
//! No `get`/`get_stream`. §3.3 says private assets are "proxy through the API with an
//! authorisation check, streaming the object" and that this "will not be built until a
//! private-asset feature actually exists". Every asset today is public, so a `get` here
//! would be an unauthenticated read path with no caller. When that feature lands it
//! arrives as a method on this trait, and the double grows with it.

use async_trait::async_trait;
use bytes::Bytes;
use uuid::Uuid;

use crate::infrastructure::storage::error::StorageError;

/// Response metadata an upload carries besides its bytes.
///
/// # Why this exists
///
/// [`ObjectStore::put`] was enough while every object was an avatar or a cover image:
/// the CDN guesses an image's type from its extension and a short cache lifetime is
/// harmless because the key is replaced on every upload. A published Bible payload is
/// the opposite case — gzip bytes at a version-keyed path, which a browser must not
/// mistake for JSON and which may be cached forever exactly because the key never
/// changes — so the upload has to say so.
///
/// Kept as an optional pair rather than as a growing header map: these are the two
/// headers a public asset needs, and an open map would invite per-call-site caching
/// policies that disagree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectMetadata {
    /// The `Content-Type` the CDN serves the object with. `None` leaves the store's own
    /// default in place, which is what every image upload wants.
    pub content_type: Option<String>,

    /// The `Cache-Control` the CDN serves the object with. `None` leaves the default.
    pub cache_control: Option<String>,
}

/// Everything the application needs from object storage.
///
/// Object-safe and `async_trait`, so callers hold `Arc<dyn ObjectStore>` and the
/// in-memory double is a drop-in substitute (§5.3).
#[async_trait]
pub trait ObjectStore: Send + Sync {
    /// Store `bytes` at `key`, replacing whatever was there.
    ///
    /// Overwrite rather than create-or-fail because every real use is an upsert: a
    /// re-uploaded avatar replaces the previous one, and the caller already deleted the
    /// old key via [`Self::delete`].
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidKey`] for a key [`ObjectKey`] rejects, and
    /// [`StorageError::Upstream`] for a store-side failure.
    async fn put(&self, key: &ObjectKey, bytes: Bytes) -> Result<(), StorageError> {
        self.put_with_metadata(key, bytes, &ObjectMetadata::default())
            .await
    }

    /// Store `bytes` at `key`, with the response metadata a public asset needs.
    ///
    /// The same upsert as [`Self::put`], plus the `Content-Type` and `Cache-Control` the
    /// CDN should serve the object with. That pair is part of the upload rather than a
    /// separate bucket-policy change because it is per object: a payload is immutable and
    /// gzip, an avatar is neither.
    ///
    /// # Errors
    ///
    /// The same two failures as [`Self::put`].
    async fn put_with_metadata(
        &self,
        key: &ObjectKey,
        bytes: Bytes,
        metadata: &ObjectMetadata,
    ) -> Result<(), StorageError>;

    /// Whether an object exists at `key`.
    ///
    /// # Errors
    ///
    /// Only for a store-side failure. A missing object is `Ok(false)` — it is an answer,
    /// not an error, and `master` getting this wrong is why
    /// [`StorageError::NotFound`] exists for the methods that genuinely cannot proceed.
    async fn exists(&self, key: &ObjectKey) -> Result<bool, StorageError>;

    /// Delete the object at `key`.
    ///
    /// Idempotent: deleting a key that is not there succeeds. `master` did this too, and
    /// it is the right call — the caller is cleaning up after a replacement upload and
    /// must not have to distinguish "the old avatar was already gone" from "deleted".
    ///
    /// # Errors
    ///
    /// Only for a store-side failure.
    async fn delete(&self, key: &ObjectKey) -> Result<(), StorageError>;

    /// The public URL an object is served from.
    ///
    /// Unsigned, by design — see the module docs. Returns `None` when no public base URL
    /// is configured, because a store with no CDN bound must not hand out URLs that 404
    /// in a way that looks like a permissions problem.
    fn public_url(&self, key: &ObjectKey) -> Option<String>;
}

/// A validated object key.
///
/// # Why a newtype instead of `&str`
///
/// §9.5 requires validating input at the boundary, and a bare `&str` makes it optional:
/// every call site would have to remember to check, and one that forgets is a path
/// traversal. Constructing one is the check — the only way to obtain an `ObjectKey` is
/// through a constructor that has already rejected `..`, a leading `/`, an empty
/// segment, and control characters.
///
/// `Display` gives the bare key back, which is what [`ObjectStore::public_url`] needs.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectKey(String);

impl ObjectKey {
    /// The key under which a user's avatar is stored.
    ///
    /// `{user_id}.{extension}` — flat, because R2 listing is prefix-based and a
    /// `avatars/{user_id}/{filename}` layout means the *old* avatar's key depends on the
    /// filename the client happened to pick, so replacing an avatar leaves the previous
    /// object behind forever. One key per user makes the delete unconditional.
    pub const AVATAR_PREFIX: &'static str = "avatars/";

    /// The key under which a broadcast's cover image is stored.
    ///
    /// `broadcasts/{broadcast_id}.{extension}` for the same reason avatars are flat:
    /// the cover is one per broadcast, so re-uploading replaces the object rather than
    /// leaving the old one behind under a filename the client chose.
    pub const BROADCAST_IMAGE_PREFIX: &'static str = "broadcasts/";

    /// The collection under which published Bible seed payloads live.
    ///
    /// Two levels deep — `bible/<code>/<version>.json.gz` — because a translation has
    /// versions, and immutable keys are what let the CDN cache a payload forever. The
    /// version is in the leaf rather than the prefix so a `bible/asv/` listing shows every
    /// version this translation ever had.
    pub const BIBLE_PREFIX: &'static str = "bible";

    /// Build a key under a named prefix, e.g. `avatars`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InvalidKey`] if the prefix or the leaf is unusable. The
    /// prefix is validated as well as the leaf because it is usually a `const` but a
    /// future caller will pass a category name from a route parameter.
    pub fn new(prefix: &str, leaf: &str) -> Result<Self, StorageError> {
        // Whitespace is trimmed as well as slashes: `"   "` is a plausible result of a
        // route parameter that was not checked for emptiness, and it would otherwise
        // become a real object key.
        let prefix = prefix.trim().trim_matches('/');
        let leaf = leaf.trim().trim_matches('/');

        if prefix.is_empty() {
            return Err(StorageError::InvalidKey(
                "the key prefix is empty".to_owned(),
            ));
        }
        validate_segment(prefix, "prefix")?;
        validate_segment(leaf, "name")?;

        Ok(Self(format!("{prefix}/{leaf}")))
    }

    /// The key for `user_id`'s avatar.
    ///
    /// # Errors
    ///
    /// Propagates [`Self::new`]'s validation failure.
    pub fn avatar(user_id: Uuid, extension: &str) -> Result<Self, StorageError> {
        Self::new(
            Self::AVATAR_PREFIX.trim_end_matches('/'),
            &format!("{user_id}.{extension}"),
        )
    }

    /// The key for `broadcast_id`'s cover image.
    ///
    /// # Errors
    ///
    /// Propagates [`Self::new`]'s validation failure.
    pub fn broadcast_image(broadcast_id: Uuid, extension: &str) -> Result<Self, StorageError> {
        Self::new(
            Self::BROADCAST_IMAGE_PREFIX.trim_end_matches('/'),
            &format!("{broadcast_id}.{extension}"),
        )
    }

    /// The key for one version of a translation's seed payload.
    ///
    /// `bible/<code>/<version>.json.gz`, the layout the mobile client's catalogue entries
    /// point at. Built here rather than by the caller so the one place that knows the
    /// shape is the one place that validates it.
    ///
    /// # Errors
    ///
    /// [`StorageError::InvalidKey`] when `code` is not two to eight ASCII lowercase
    /// letters or digits (the codes the catalogue uses — `kjv`, `web`, `asv`), or when
    /// `version` is zero: a payload version starts at one, and `0.json.gz` would be a key
    /// nothing in the app can describe.
    pub fn bible_payload(code: &str, version: u32) -> Result<Self, StorageError> {
        let code_ok = (2..=8).contains(&code.len())
            && code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
        if !code_ok {
            return Err(StorageError::InvalidKey(format!(
                "`{code}` is not a translation code (two to eight lowercase letters or digits)"
            )));
        }
        if version == 0 {
            return Err(StorageError::InvalidKey(
                "a payload version starts at 1".to_owned(),
            ));
        }

        Ok(Self(format!(
            "{}/{}/{}.json.gz",
            Self::BIBLE_PREFIX,
            code,
            version
        )))
    }

    /// The key as a plain string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Rebuild a key from a string that was stored earlier.
    ///
    /// `split_once` rather than assuming the prefix: the key is data at this point, and
    /// a key that does not round-trip (nested, empty, trailing slash) returns `None`
    /// instead of being silently mangled into a *different* object — which is how a
    /// cleanup path deletes the wrong file.
    ///
    /// Lives here rather than beside either caller because both the avatar and the
    /// broadcast-cover paths have to address an object the database named, and a second
    /// copy of this rule is a second chance to get the `None` case wrong.
    #[must_use]
    pub fn from_stored(key: &str) -> Option<Self> {
        let (prefix, leaf) = key.split_once('/')?;
        Self::new(prefix, leaf).ok()
    }

    /// Whether this key is the cover image for `broadcast_id`.
    ///
    /// The same use as [`Self::is_avatar_of`]: a caller replacing a cover can skip a
    /// pointless delete round-trip when the key it is about to overwrite is the one
    /// the row already names.
    #[must_use]
    pub fn is_broadcast_image_of(&self, broadcast_id: Uuid) -> bool {
        let Some(rest) = self.0.strip_prefix(Self::BROADCAST_IMAGE_PREFIX) else {
            return false;
        };
        rest.strip_prefix(&broadcast_id.to_string())
            .is_some_and(|extension| extension.starts_with('.'))
    }

    /// Whether this key is the avatar for `user_id`.
    ///
    /// Exists so the caller replacing an avatar can decide whether the key it is about
    /// to overwrite is the same one, and skip a pointless delete round-trip.
    #[must_use]
    pub fn is_avatar_of(&self, user_id: Uuid) -> bool {
        let Some(rest) = self.0.strip_prefix(Self::AVATAR_PREFIX) else {
            return false;
        };
        rest.strip_prefix(&user_id.to_string())
            .is_some_and(|extension| extension.starts_with('.'))
    }
}

impl std::fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Reject anything that could escape the prefix or confuse a URL.
///
/// Split out so [`ObjectKey::new`] and the tests can share one definition — the rules are
/// the security property, and duplicating them is how they drift.
fn validate_segment(segment: &str, what: &str) -> Result<(), StorageError> {
    if segment.is_empty() {
        return Err(StorageError::InvalidKey(format!("the key {what} is empty")));
    }
    if segment.contains("..") {
        return Err(StorageError::InvalidKey(format!(
            "the key {what} must not contain `..`"
        )));
    }
    if segment.contains('/') {
        return Err(StorageError::InvalidKey(format!(
            "the key {what} must not contain `/`"
        )));
    }
    if segment.chars().any(char::is_control) {
        return Err(StorageError::InvalidKey(format!(
            "the key {what} must not contain control characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Tests for key validation — the §9.5 boundary.

    use super::*;

    #[test]
    fn a_key_is_the_prefix_and_the_leaf_joined_by_one_slash() {
        let key = ObjectKey::new("broadcasts", "42.png").expect("a valid key");

        assert_eq!(key.as_str(), "broadcasts/42.png");
        assert_eq!(key.to_string(), "broadcasts/42.png");
    }

    #[test]
    fn a_trailing_slash_on_the_prefix_does_not_double_it() {
        // `format!("{prefix}/{leaf}")` on an untrimmed prefix produces `a//b`, which is a
        // *different* object key in S3. This is the regression that trimming prevents.
        let key = ObjectKey::new("avatars/", "1.png").expect("a valid key");

        assert_eq!(key.as_str(), "avatars/1.png");
        assert!(!key.as_str().contains("//"));
    }

    #[test]
    fn a_traversal_in_the_leaf_is_rejected() {
        // The reason this type exists. `avatars/../../../etc/passwd` is a legal string and
        // a catastrophic key.
        let error = ObjectKey::new("avatars", "../../etc/passwd").expect_err("must reject");

        assert!(
            error.to_string().contains(".."),
            "the message must say what was wrong: {error}"
        );
    }

    #[test]
    fn a_traversal_in_the_prefix_is_rejected_too() {
        let error = ObjectKey::new("../secrets", "x").expect_err("must reject");

        assert!(matches!(error, StorageError::InvalidKey(_)));
    }

    #[test]
    fn an_embedded_slash_in_the_leaf_is_rejected() {
        // Otherwise `ObjectKey::new("avatars", "a/b")` silently produces a nested key and
        // the caller's "one object per user" assumption breaks.
        assert!(ObjectKey::new("avatars", "a/b").is_err());
    }

    #[test]
    fn an_empty_prefix_or_leaf_is_rejected() {
        assert!(ObjectKey::new("", "a.png").is_err());
        assert!(ObjectKey::new("avatars", "").is_err());
        assert!(ObjectKey::new("   ", "a.png").is_err());
    }

    #[test]
    fn control_characters_are_rejected() {
        // A newline in a key ends up in a `Content-Disposition` header and a log line.
        assert!(ObjectKey::new("avatars", "a\nb.png").is_err());
        assert!(ObjectKey::new("avatars", "a\0b.png").is_err());
    }

    #[test]
    fn an_avatar_key_is_one_per_user_and_recognisable() {
        // Flat, not `avatars/{user_id}/{filename}`: the old layout's key depended on the
        // client's filename, so replacing an avatar orphaned the previous object.
        let user_id = Uuid::from_u128(7);
        let key = ObjectKey::avatar(user_id, "png").expect("a valid key");

        assert_eq!(
            key.as_str(),
            format!("avatars/{user_id}.png"),
            "one flat key per user: the id and the extension"
        );
        assert!(key.is_avatar_of(user_id));
        assert!(!key.is_avatar_of(Uuid::from_u128(8)));
    }

    #[test]
    fn a_different_extension_is_a_different_avatar_key() {
        // Worth pinning: if this ever collapses, replacing a `.png` avatar with a `.jpg`
        // would leave the old object readable and never delete it.
        let user_id = Uuid::from_u128(1);

        assert_ne!(
            ObjectKey::avatar(user_id, "png").expect("png"),
            ObjectKey::avatar(user_id, "jpg").expect("jpg")
        );
    }

    #[test]
    fn a_bible_payload_key_is_collection_code_version_and_extension() {
        // The path the mobile catalogue points at. Pinned as a whole string because
        // changing any part of it silently turns every published payload into a 404.
        let key = ObjectKey::bible_payload("asv", 2).expect("a valid key");

        assert_eq!(key.as_str(), "bible/asv/2.json.gz");
    }

    #[test]
    fn a_translation_code_that_is_not_a_code_is_rejected() {
        // The code reaches a URL path segment and an object key. Rejecting it here is
        // what keeps a catalogue typo from publishing somewhere nobody looks.
        for code in ["", "A", "ASV", "a", "../etc", "as v", "asv.json", "asv-1"] {
            assert!(
                ObjectKey::bible_payload(code, 1).is_err(),
                "{code:?} must not become a key"
            );
        }
    }

    #[test]
    fn a_zero_version_is_rejected() {
        // A payload version starts at one: `0.json.gz` is a key the app's ledger can
        // never describe, so publishing it would be invisible.
        assert!(ObjectKey::bible_payload("kjv", 0).is_err());
    }

    #[test]
    fn a_broadcast_image_key_is_not_mistaken_for_an_avatar() {
        let key = ObjectKey::new("broadcasts", "9.png").expect("a valid key");

        assert!(!key.is_avatar_of(Uuid::from_u128(9)));
    }
}
