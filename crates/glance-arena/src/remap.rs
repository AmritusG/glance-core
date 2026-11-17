// SPDX-License-Identifier: MIT
//! Maps a path on this computer to the same file as the Arena machine sees it (Arena on another
//! computer: `/Volumes/X/clip.mov` here may be `/Volumes/Y/clip.mov` there). Prefixes match whole
//! path components only; the longest matching local prefix wins.

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathRemap {
    #[serde(with = "crate::swift_json::uuid_upper")]
    pub id: Uuid,
    pub local_prefix: String,
    pub remote_prefix: String,
}

impl PathRemap {
    pub fn new(local_prefix: impl Into<String>, remote_prefix: impl Into<String>) -> Self {
        PathRemap { id: Uuid::new_v4(), local_prefix: local_prefix.into(), remote_prefix: remote_prefix.into() }
    }

    /// Trailing slashes dropped, except a lone "/".
    fn trimmed(prefix: &str) -> String {
        let mut p = prefix.to_string();
        while p.chars().count() > 1 && p.ends_with('/') {
            p.pop();
        }
        p
    }

    /// The remapped path, or `None` when `path` is not under the local prefix. Prefixes compare
    /// by canonical equivalence (an NFD path matches an NFC prefix), as Swift strings do.
    pub fn apply(&self, path: &str) -> Option<String> {
        let local = nfc(&Self::trimmed(&self.local_prefix));
        let remote = Self::trimmed(&self.remote_prefix);
        if local.is_empty() {
            return None;
        }
        let path = nfc(path);
        if path == local {
            return Some(remote);
        }
        let with_slash = if local == "/" { "/".to_string() } else { format!("{local}/") };
        let rest = path.strip_prefix(&with_slash)?;
        Some(if remote == "/" { format!("/{rest}") } else { format!("{remote}/{rest}") })
    }
}

fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// Longest matching local prefix wins; unmatched paths pass through.
pub fn remap(remaps: &[PathRemap], path: &str) -> String {
    let mut candidates: Vec<&PathRemap> = remaps.iter().filter(|r| r.apply(path).is_some()).collect();
    candidates.sort_by_key(|r| std::cmp::Reverse(PathRemap::trimmed(&r.local_prefix).chars().count()));
    candidates.first().and_then(|r| r.apply(path)).unwrap_or_else(|| path.to_string())
}
