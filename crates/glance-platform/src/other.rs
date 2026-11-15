//! Systems without a shell of their own yet (iOS — GlancEhr's own wave; Linux — not a target).

use crate::{NowPlaying, PlatformError};
use std::path::{Path, PathBuf};

/// No folder listing here yet (GlancEhr's wave brings iOS's).
pub fn movie_files(_dir: &Path) -> Result<Vec<PathBuf>, PlatformError> {
    Err(PlatformError::Unsupported("folder listing"))
}

/// No media keys here.
#[derive(Debug)]
pub struct MediaKeys;

impl MediaKeys {
    pub fn register() -> Result<MediaKeys, PlatformError> {
        Err(PlatformError::Unsupported("media keys"))
    }

    pub fn now_playing(&self, _info: &NowPlaying) {}
}
