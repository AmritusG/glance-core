//! Systems without a shell of their own yet (iOS — GlancEhr's own wave; Linux — not a target).

use crate::{NowPlaying, PlatformError};

/// No media keys here.
#[derive(Debug)]
pub struct MediaKeys;

impl MediaKeys {
    pub fn register() -> Result<MediaKeys, PlatformError> {
        Err(PlatformError::Unsupported("media keys"))
    }

    pub fn now_playing(&self, _info: &NowPlaying) {}
}
