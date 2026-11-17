//! What the catalogue handler needs from the application.
//!
//! One value: the CDN base its payload URLs are built from. No pool, no Redis, no object
//! store — the handler never uploads or reads an object, it only names where one is
//! served from, so handing it a store would be handing it a capability it has no use for.

/// The catalogue's state.
///
/// `payload_base_url` is `None` when this deployment has no object storage configured
/// (`STORAGE_ENABLED=false`, or no `STORAGE_PUBLIC_URL`), which is a real deployment: the
/// API still boots and serves everything else, and this endpoint says so with a 503 rather
/// than answering URLs that would 404 behind a CDN that is not bound.
#[derive(Clone, Debug)]
pub struct BibleState {
    payload_base_url: Option<String>,
}

impl BibleState {
    /// Builds the state from a configured public URL, if this deployment has one.
    ///
    /// Trimmed and normalised here, once, rather than in the URL builder per request: a
    /// `STORAGE_PUBLIC_URL` written with a trailing slash, or as an empty string, would
    /// otherwise produce `...//bible/...` or `/bible/...` URLs that look valid and 404.
    #[must_use]
    pub fn new(public_url: Option<String>) -> Self {
        Self {
            payload_base_url: public_url
                .map(|url| url.trim().trim_end_matches('/').to_owned())
                .filter(|url| !url.is_empty()),
        }
    }

    /// The CDN base payload URLs are built from, or `None` when storage is not configured.
    #[must_use]
    pub fn payload_base_url(&self) -> Option<&str> {
        self.payload_base_url.as_deref()
    }
}

#[cfg(test)]
mod tests {
    //! Normalisation, because the failure it prevents is a URL that renders and 404s.

    use super::*;

    #[test]
    fn a_trailing_slash_is_trimmed() {
        assert_eq!(
            BibleState::new(Some("https://cdn.meno.app/".to_owned())).payload_base_url(),
            Some("https://cdn.meno.app")
        );
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        // A `.env` line pasted with a trailing space is common enough to normalise.
        assert_eq!(
            BibleState::new(Some("  https://cdn.meno.app  ".to_owned())).payload_base_url(),
            Some("https://cdn.meno.app")
        );
    }

    #[test]
    fn an_unconfigured_or_blank_url_is_none() {
        // `None` and blank mean the same thing to the handler — no URL can be built — so
        // they normalise to the same answer rather than one of them producing `/bible/...`.
        assert_eq!(BibleState::new(None).payload_base_url(), None);
        assert_eq!(
            BibleState::new(Some(String::new())).payload_base_url(),
            None
        );
        assert_eq!(
            BibleState::new(Some("   ".to_owned())).payload_base_url(),
            None
        );
    }
}
