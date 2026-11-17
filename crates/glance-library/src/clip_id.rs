//! The content id `clip_id`: Swift MaCrate's `ClipHasher.computeClipID` (CrateKit/ClipHasher.swift), the key of
//! `clips`, sidecars, stash entries, `clip_alpha` and the measurements — byte for byte the same string, so a library
//! Swift MaCrate or iCrate wrote is read and written back unchanged.
//!
//! `lowercase hex(SHA-256(first min(size, 1 MiB) bytes ‖ size as an 8-byte little-endian i64)) + ":" + size`.
//! The tail of the file is never read; a move, a rename or an mtime change keeps the id.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// How many leading bytes are hashed (`1024 * 1024`).
pub const HASHED_PREFIX: usize = 1 << 20;

/// The clip id of the file at `path`, reading at most [`HASHED_PREFIX`] bytes in a streaming read.
///
/// As Swift: the file is opened (following a symbolic link), its first MiB read, and the size taken from the
/// path's own attributes afterwards — `FileManager.attributesOfItem`, which does not follow a link, so a link's
/// id carries the link's own size. A missing or unreadable file is an error (Swift throws).
pub fn clip_id(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut head = Vec::with_capacity(HASHED_PREFIX.min(1 << 16));
    (&mut file).take(HASHED_PREFIX as u64).read_to_end(&mut head)?;
    let size = i64::try_from(std::fs::symlink_metadata(path)?.len()).map_err(std::io::Error::other)?;
    Ok(clip_id_of(&head, size))
}

/// The clip id of `head` (the file's first `min(size, 1 MiB)` bytes) and the file's `size`.
pub fn clip_id_of(head: &[u8], size: i64) -> String {
    let mut hash = Sha256::new();
    hash.update(head);
    hash.update(size.to_le_bytes());
    let mut id = String::with_capacity(64 + 21);
    for b in hash.finalize() {
        id.push(char::from(HEX[usize::from(b >> 4)]));
        id.push(char::from(HEX[usize::from(b & 15)]));
    }
    id.push(':');
    id.push_str(&size.to_string());
    id
}

const HEX: &[u8; 16] = b"0123456789abcdef";

/// The file size a clip id carries, as Swift's `ClipMetadata.fileSizeFromClipID` reads it back
/// (`clipID.split(separator: ":", maxSplits: 1).last` parsed as `Int64`): the text after the first `:` that ends
/// a non-empty piece — empty pieces are dropped, so leading colons are skipped and an id without a usable suffix
/// falls back to its first piece. `None` when that text is not a decimal `i64` (an optional `+` / `-` sign, ASCII
/// digits, no spaces, no overflow).
pub fn size_from_clip_id(id: &str) -> Option<i64> {
    let rest = id.trim_start_matches(':');
    let last = match rest.split_once(':') {
        Some((_, tail)) if !tail.is_empty() => tail,
        Some((first, _)) => first,
        None => rest,
    };
    if last.is_empty() {
        return None;
    }
    last.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_id_is_lowercase_hex_colon_size() {
        let id = clip_id_of(&[0xaa, 0xbb, 0xcc], 3);
        let (hex, size) = id.split_once(':').unwrap_or_default();
        assert_eq!(hex.len(), 64);
        assert!(hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        assert_eq!(size, "3");
    }

    #[test]
    fn the_size_is_mixed_into_the_digest() {
        // Same leading bytes, one byte longer file: both the digest and the suffix change (ClipHasherTests).
        let a = clip_id_of(&[0x42; 3], 3);
        let b = clip_id_of(&[0x42; 3], 4);
        assert_ne!(a.split_once(':').map(|p| p.0), b.split_once(':').map(|p| p.0));
    }

    #[test]
    fn known_vector() {
        // SHA-256 of the 8 bytes of 0i64 (an empty file): python3 -c "import hashlib;print(hashlib.sha256(bytes(8)).hexdigest())"
        assert_eq!(clip_id_of(&[], 0), "af5570f5a1810b7af78caf4bc70a660f0df51e42baf91d4de5b2328de0e83dfc:0");
    }

    #[test]
    fn the_size_reads_back_as_swift_splits() {
        let cases: [(&str, Option<i64>); 16] = [
            ("abc:123", Some(123)),
            ("abc", None),
            ("123", Some(123)),
            (":123", Some(123)),
            ("::123", Some(123)),
            ("abc:", None),
            ("5:", Some(5)),
            ("a:b:c", None),
            ("a::5", None),
            ("a:+5", Some(5)),
            ("a:-5", Some(-5)),
            ("a: 5", None),
            ("a:9223372036854775807", Some(i64::MAX)),
            ("a:9223372036854775808", None),
            ("", None),
            (":", None),
        ];
        for (id, want) in cases {
            assert_eq!(size_from_clip_id(id), want, "{id:?}");
        }
    }
}
