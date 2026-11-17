//! The catalogue's wire shape.
//!
//! Six fields, and deliberately no more. A client needs to identify a translation
//! (`code`, `name`), decide whether it wants it (`version`, `bytes`) and verify what it
//! downloaded (`sha256`, `payloadUrl`) — anything else would be a field the app has to
//! ignore. The attribution text a translation carries lives inside its payload, which is
//! where the client reads it from once the payload is on the device.

use serde::Serialize;

use crate::infrastructure::storage::StorageError;
use crate::modules::bible::catalogue::{self, CatalogueEntry};

/// One translation a client may download.
///
/// Borrowed strings rather than `String`s because every value comes from the `const`
/// catalogue — the response is a projection of a table, not a row that was assembled per
/// request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BibleTranslationDto {
    /// The translation code, e.g. `asv`. What the client stores its verses under.
    pub code: &'static str,

    /// The display name, e.g. `American Standard Version`.
    pub name: &'static str,

    /// The payload's version. The client compares it with what its seed ledger holds.
    pub version: u32,

    /// The payload's exact size in bytes, for progress and for a first sanity check.
    pub bytes: u64,

    /// Lowercase hex SHA-256 of the payload, verified after the download.
    pub sha256: &'static str,

    /// Absolute URL the payload is served from, built from `STORAGE_PUBLIC_URL`.
    ///
    /// Absolute rather than a key the client joins itself: the CDN base differs per
    /// deployment (`https://cdn.meno.app` in production, the object server's own URL in
    /// development), and a client that glued its own path on would be right in one of
    /// them.
    pub payload_url: String,
}

impl BibleTranslationDto {
    /// The wire row for `entry`, served under `public_url`.
    ///
    /// # Errors
    ///
    /// [`StorageError::InvalidKey`] if the entry's code or version cannot become a key —
    /// which a test pins against the real table, so this is a guard rather than a path a
    /// request can take.
    pub fn for_entry(entry: &CatalogueEntry, public_url: &str) -> Result<Self, StorageError> {
        Ok(Self {
            code: entry.code,
            name: entry.name,
            version: entry.version,
            bytes: entry.bytes,
            sha256: entry.sha256,
            payload_url: catalogue::payload_url(public_url, entry)?,
        })
    }
}

#[cfg(test)]
mod tests {
    //! The wire contract: the six keys, their names, and nothing a client has to skip.

    use super::*;

    fn asv() -> &'static CatalogueEntry {
        catalogue::entry("asv").expect("asv is catalogued")
    }

    #[test]
    fn the_row_serialises_with_the_six_documented_keys() {
        // Exact equality, not a subset: an extra key here would be a field the Flutter and
        // Next.js clients carry around without reading.
        let row = BibleTranslationDto::for_entry(asv(), "https://cdn.test").expect("a row");

        assert_eq!(
            serde_json::to_value(row).expect("serialisable"),
            serde_json::json!({
                "code": "asv",
                "name": "American Standard Version",
                "version": 1,
                "bytes": 1_331_817_u64,
                "sha256": "556643d5608fcc5de1004daea48f822a3ba3ce38b166f2c3e9ef3253e75cc9dc",
                "payloadUrl": "https://cdn.test/bible/asv/1.json.gz",
            })
        );
    }

    #[test]
    fn the_payload_url_is_absolute() {
        // The client downloads this string directly, so a relative path would have to be
        // joined against the API — a different origin from the CDN.
        let row = BibleTranslationDto::for_entry(asv(), "https://cdn.meno.app/").expect("a row");

        assert_eq!(row.payload_url, "https://cdn.meno.app/bible/asv/1.json.gz");
    }
}
