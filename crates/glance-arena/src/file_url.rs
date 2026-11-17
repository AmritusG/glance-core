// SPDX-License-Identifier: MIT
//! The `file:///` body Arena's `/clips/{C}/open` expects (Arena's swagger.yaml, FileUrl): three
//! slashes, forward slashes, every special character percent-encoded (space → %20, # → %23,
//! % → %25, & → %26, ? → %3F); non-ASCII as percent-encoded UTF-8.

use unicode_normalization::UnicodeNormalization;

/// Encoding and decoding of Arena's file URLs.
pub struct ArenaFileURL;

impl ArenaFileURL {
    /// RFC 3986 unreserved characters plus the path separator.
    fn allowed(b: u8) -> bool {
        b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/')
    }

    /// `/Volumes/Media/My Videos/a b.mov` → `file:///Volumes/Media/My%20Videos/a%20b.mov`. The
    /// path is NFC-normalised first so a Finder (NFD) name and a typed name encode identically.
    pub fn encode(path: &str) -> String {
        let normalized: String = path.nfc().collect();
        let absolute = if normalized.starts_with('/') { normalized } else { format!("/{normalized}") };
        let mut out = String::with_capacity(absolute.len() + 8);
        out.push_str("file://");
        for b in absolute.bytes() {
            if Self::allowed(b) {
                out.push(char::from(b));
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        }
        out
    }

    /// Inverse of [`ArenaFileURL::encode`], for log lines and slot checks. `None` when `url` is
    /// not a `file://` URL, or its escapes are malformed or not UTF-8.
    pub fn decode(url: &str) -> Option<String> {
        let rest = url.strip_prefix("file://")?;
        let bytes = rest.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'%' {
                let hi = bytes.get(i + 1).and_then(|c| char::from(*c).to_digit(16))?;
                let lo = bytes.get(i + 2).and_then(|c| char::from(*c).to_digit(16))?;
                out.push(u8::try_from(hi * 16 + lo).ok()?);
                i += 3;
            } else {
                out.push(b);
                i += 1;
            }
        }
        String::from_utf8(out).ok()
    }
}
