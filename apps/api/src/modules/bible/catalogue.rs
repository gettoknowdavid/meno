//! The translations this deployment publishes, and where each payload lives.
//!
//! # The table is the source of truth
//!
//! [`TRANSLATIONS`] names every payload the endpoint advertises, with the two values a
//! client verifies after downloading: the byte length and the SHA-256. Both were computed
//! from the file that was published — [`super::publish`] refuses to upload a payload whose
//! digest disagrees with the entry, which is what keeps the table from describing one file
//! while the bucket holds another.
//!
//! Adding a translation is therefore three steps, in this order: convert the payload
//! (`tools/bible_converter --seed-output`), add its entry here with the file's size and
//! digest, and run `cargo run --bin publish-bible -- --code <code> --payload <path>`.
//! Publishing first would upload an object nothing describes; publishing a file whose
//! entry was not updated fails loudly instead of drifting.
//!
//! # Versions, not overwrites
//!
//! `version` is the payload's own version, and it is part of the object key
//! (`bible/<code>/<version>.json.gz`) because the CDN caches those keys for a year as
//! immutable. A corrected text is version 2 — a new key, a new entry — never a rewrite of
//! version 1. The client's seed ledger compares this number with what it holds, which is
//! what makes "an update is available" a comparison rather than a download every launch.

use crate::infrastructure::storage::{ObjectKey, StorageError};

/// One translation this deployment publishes.
///
/// Every field is `&'static str` or a number because the whole table is a `const`: a
/// catalogue entry is data from a release, not from a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatalogueEntry {
    /// The translation code the payload stores its verses under, e.g. `asv`.
    ///
    /// Two to eight lowercase ASCII letters or digits — [`ObjectKey::bible_payload`]
    /// enforces it, because this value becomes a URL path segment and an object key.
    pub code: &'static str,

    /// The name a client shows before the download, e.g. `American Standard Version`.
    pub name: &'static str,

    /// The payload's version. Bumping it publishes a *new* object; it never rewrites one.
    pub version: u32,

    /// The payload's exact byte length, for progress and for a cheap first check.
    pub bytes: u64,

    /// The payload's SHA-256, lowercase hex, so a client can verify what it downloaded.
    pub sha256: &'static str,
}

impl CatalogueEntry {
    /// Where this entry's payload lives.
    ///
    /// # Errors
    ///
    /// [`StorageError::InvalidKey`] for a code or version
    /// [`ObjectKey::bible_payload`] refuses. A test pins that every entry in
    /// [`TRANSLATIONS`] builds one, so this is a guard rather than a runtime possibility.
    pub fn payload_key(&self) -> Result<ObjectKey, StorageError> {
        ObjectKey::bible_payload(self.code, self.version)
    }
}

/// Every payload this deployment publishes, ordered by code.
///
/// Three today — the same three the app has been bundling — because they are the payloads
/// that exist. A translation is added here only once its file is ready to publish.
pub const TRANSLATIONS: &[CatalogueEntry] = &[
    CatalogueEntry {
        code: "asv",
        name: "American Standard Version",
        version: 1,
        bytes: 1_331_817,
        sha256: "556643d5608fcc5de1004daea48f822a3ba3ce38b166f2c3e9ef3253e75cc9dc",
    },
    CatalogueEntry {
        code: "kjv",
        name: "Authorized King James Version",
        version: 1,
        bytes: 1_357_007,
        sha256: "5f8346b004e40bf164fbb9a1076b651fd8d78984551b86b087fb1eca4ef74cb9",
    },
    CatalogueEntry {
        code: "web",
        name: "World English Bible",
        version: 1,
        bytes: 1_309_826,
        sha256: "cebd01231a53609f7fb9a0e720af1df39b351c99c41e115f4d5721f2a1dafa43",
    },
];

/// The entry for `code`, or `None` when this deployment does not publish it.
///
/// Used by `publish-bible` to check a payload against the table before uploading it.
#[must_use]
pub fn entry(code: &str) -> Option<&'static CatalogueEntry> {
    TRANSLATIONS.iter().find(|entry| entry.code == code)
}

/// The absolute URL `entry`'s payload is served from, under `public_url`.
///
/// A string join, not a signature: §3.3 binds a custom domain to the bucket so public
/// assets are read straight from the CDN. The base is trimmed of one trailing slash
/// because `STORAGE_PUBLIC_URL` is written both ways in the wild and `//` is a different
/// URL to a cache.
///
/// # Errors
///
/// Whatever [`CatalogueEntry::payload_key`] reports — see it for why that cannot happen
/// for a table entry.
pub fn payload_url(public_url: &str, entry: &CatalogueEntry) -> Result<String, StorageError> {
    let key = entry.payload_key()?;
    Ok(format!(
        "{}/{}",
        public_url.trim_end_matches('/'),
        key.as_str()
    ))
}

#[cfg(test)]
mod tests {
    //! The table's own contract. These are the assertions that make a `const` catalogue
    //! as safe as a validated one: shape, uniqueness, order, and the URL it produces.

    use super::*;

    #[test]
    fn every_entry_builds_a_payload_key() {
        // The guard that turns `bible_payload`'s `Result` into a compile-of-the-table
        // property: a code with a capital or a version of zero would publish to a key the
        // endpoint cannot describe.
        for entry in TRANSLATIONS {
            let key = entry
                .payload_key()
                .unwrap_or_else(|error| panic!("{} v{}: {error}", entry.code, entry.version));

            assert_eq!(
                key.as_str(),
                format!("bible/{}/{}.json.gz", entry.code, entry.version)
            );
        }
    }

    #[test]
    fn codes_are_unique_and_sorted() {
        // Unique, because the object key and the client's ledger are keyed by code: two
        // entries with one code would answer the same URL with different hashes.
        // Sorted, so the endpoint's order is stable and a diff of this table reads as a
        // change rather than a reshuffle.
        let codes: Vec<&str> = TRANSLATIONS.iter().map(|entry| entry.code).collect();
        let mut sorted = codes.clone();
        sorted.sort_unstable();

        assert_eq!(codes, sorted, "the catalogue is ordered by code");

        let unique: std::collections::HashSet<&&str> = codes.iter().collect();
        assert_eq!(unique.len(), codes.len(), "duplicate codes: {codes:?}");
    }

    #[test]
    fn every_hash_is_a_sha256_in_lowercase_hex() {
        // The client compares this string byte for byte. An uppercase or truncated hash
        // would make every download fail verification, and the failure would look like a
        // corrupted payload rather than a typo in this table.
        for entry in TRANSLATIONS {
            assert_eq!(
                entry.sha256.len(),
                64,
                "{}: a SHA-256 is 64 hex characters, got {:?}",
                entry.code,
                entry.sha256
            );
            assert!(
                entry
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "{}: {:?} is not lowercase hex",
                entry.code,
                entry.sha256
            );
        }
    }

    #[test]
    fn every_entry_describes_a_real_payload() {
        // The two numbers a client needs to verify a download. Zero would let an empty
        // file pass, and a version of zero would be a key the ledger cannot express.
        for entry in TRANSLATIONS {
            assert!(entry.bytes > 0, "{}: a payload has bytes", entry.code);
            assert!(entry.version >= 1, "{}: versions start at 1", entry.code);
            assert!(!entry.name.is_empty(), "{}: a name is shown", entry.code);
        }
    }

    #[test]
    fn a_payload_url_is_the_cdn_base_and_the_key() {
        let entry = entry("asv").expect("asv is catalogued");

        assert_eq!(
            payload_url("https://cdn.meno.app", entry).expect("a URL"),
            "https://cdn.meno.app/bible/asv/1.json.gz"
        );
        // A base written with a trailing slash must not produce `//`, which a CDN treats
        // as a different path.
        assert_eq!(
            payload_url("https://cdn.meno.app/", entry).expect("a URL"),
            "https://cdn.meno.app/bible/asv/1.json.gz"
        );
    }

    #[test]
    fn an_unknown_code_is_not_an_entry() {
        assert!(entry("niv").is_none());
        assert!(entry("ASV").is_none(), "codes are lowercase");
    }
}
