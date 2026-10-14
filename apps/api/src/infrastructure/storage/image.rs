//! Image sniffing: what a set of bytes actually is, from its magic number.
//!
//! # Why this is shared, and not a method on the avatar path
//!
//! Two features now accept an uploaded image — a profile avatar (`profile::storage`)
//! and a broadcast cover (`broadcast::storage`) — and both must agree on exactly which
//! bytes are an image and what extension the stored key should carry. A second copy of
//! these six `starts_with` calls would be a second place for the rule to drift, and the
//! drift would be a security property: the day one copy learns that `RIFF` alone is not
//! WebP and the other does not, one path starts storing non-images.
//!
//! # Why sniffing at all
//!
//! `Content-Type` and the filename are the client's word for what the file is; the
//! first bytes are the file's own. Checking magic numbers before a byte is written
//! means an `.exe` renamed to `.png` never becomes an object, and — more practically —
//! a client bug that posts an HTML error page cannot be stored and then served from
//! the CDN as an image.

/// The file's real extension, from its magic number.
///
/// `None` for anything else — every caller turns that into a 422 before any I/O.
#[must_use]
pub fn sniff_image_extension(bytes: &[u8]) -> Option<&'static str> {
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
    //! These are the boundary between "a file" and "a 422", so they are checked with the
    //! bytes a real encoder writes rather than with fixtures that only look plausible to
    //! the function being tested. Both upload paths share them by sharing this function.

    use super::*;

    #[test]
    fn sniff_recognises_the_three_allowed_formats() {
        assert_eq!(
            sniff_image_extension(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]),
            Some("jpg")
        );
        assert_eq!(
            sniff_image_extension(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00]),
            Some("png")
        );
        let mut webp = b"RIFF\x00\x00\x00\x00WEBPVP8 ".to_vec();
        webp.extend_from_slice(&[0x00; 4]);
        assert_eq!(sniff_image_extension(&webp), Some("webp"));
    }

    #[test]
    fn sniff_refuses_everything_else() {
        assert_eq!(sniff_image_extension(&[]), None);
        assert_eq!(
            sniff_image_extension(b"<html><body>login required</body></html>"),
            None
        );
        assert_eq!(sniff_image_extension(b"GIF89a"), None);
        assert_eq!(sniff_image_extension(b"RIFF\x00\x00\x00\x00WAVEfmt "), None);
        // A truncated PNG header is not a PNG.
        assert_eq!(sniff_image_extension(&[0x89, b'P', b'N', b'G']), None);
    }
}
