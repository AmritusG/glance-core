//! Content routing, and what the viewer says when nothing plays a file.
//!
//! Swift Glance routes by what the file holds, never by its name (`PlayerModel.open`,
//! glance/Sources/Glance/PlayerModel.swift:209-229): DXV first, then HAP, then the OS decoder; a
//! file the OS decoder cannot open shows a message in the window instead of a black picture
//! (`AVPlaybackController.inspect`, glance/Sources/Glance/AVPlaybackController.swift:84-105).
//! [`route`] is that decision over DXVKit Plus's probe (which since v0.9.0 also picks the track as
//! Swift does: the first DXV track anywhere, else HAP when the first video track is HAP), and
//! [`NotPlayable::message`] is that message, word for word (`dxvkit_plus_core::names`, Swift
//! Glance's table).
//!
//! The Rust family plays more than the Swift apps (Amrit, 2026-11-14: add, then prune): HAP R
//! (`Hap7`), HAP HDR (`HapH`) and DXVKit Plus's own LMBD route to their decoders here, where Swift
//! Glance shows "Hap7 in MOV is not supported".

use crate::{MediaError, MediaInfo, SourceKind, import_options};
use dxvkit_plus_core::container::{Codec, Movie};
use dxvkit_plus_core::names::unsupported_message;
use dxvkit_plus_core::source::{self, FrameSource};
use std::path::Path;

/// Which engine plays a clip (Swift Glance's `ActivePath`, plus LMBD).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Route {
    /// `DXD3` / `DXD4` / `DXDI` sample entries; the packet decides the variant and the generation.
    Dxv,
    /// `Hap1` / `Hap5` / `HapY` / `HapM` / `HapA`, and `Hap7` / `HapH`.
    Hap,
    /// Everything else the OS decodes (H.264, HEVC, ProRes, Motion JPEG).
    Native,
    /// DXVKit Plus's LMBD (`.lmbd`).
    Lmbd,
}

/// A file that will not play: the line the viewer shows, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotPlayable {
    /// Swift Glance's wording: `<codec> in <CONTAINER> is not supported`, `<CONTAINER> files are
    /// not supported`, `This DXV file could not be opened` or `This HAP file could not be opened`.
    pub message: String,
    /// What failed underneath (for the log, not for the window).
    pub cause: MediaError,
}

impl std::fmt::Display for NotPlayable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for NotPlayable {}

/// A routed clip: what the probe learned and which engine plays it.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub info: MediaInfo,
    pub route: Route,
}

impl Clip {
    /// A frame-exact decoder for the clip. A frame's chunks decompress on worker threads.
    pub fn open(&self) -> Result<Box<dyn FrameSource>, NotPlayable> {
        source::open(&self.info, &import_options(), true).map_err(|cause| {
            let message = match self.route {
                Route::Dxv => "This DXV file could not be opened".to_string(),
                Route::Hap => "This HAP file could not be opened".to_string(),
                Route::Native | Route::Lmbd => unsupported_message(Path::new(&self.info.path)),
            };
            NotPlayable { message, cause }
        })
    }
}

/// Route `path` by its content. Every failure — a missing file, zero bytes, garbage, a truncated
/// movie, a codec nothing here decodes — is a [`NotPlayable`] carrying the line to show.
pub fn route(path: &Path) -> Result<Clip, NotPlayable> {
    let not = |cause: MediaError| NotPlayable { message: unsupported_message(path), cause };
    let Some(utf8) = path.to_str() else { return Err(not(MediaError::Invalid("the path is not UTF-8".into()))) };
    if path.is_dir() {
        return Err(not(MediaError::Unsupported("folders are not opened as clips".into())));
    }
    let info = source::probe(utf8, &import_options()).map_err(not)?;
    let route = match info.kind {
        SourceKind::Dxv => {
            // Swift Glance (DXVKit's demuxer) takes a DXV track only when its first packet states a
            // variant; otherwise the file falls through to the message.
            first_dxv_packet(path).map_err(not)?;
            Route::Dxv
        }
        SourceKind::Hap => Route::Hap,
        SourceKind::Native => Route::Native,
        SourceKind::Lmbd => Route::Lmbd,
        // Stills are Glance G1's to decide; Swift Glance does not open them.
        SourceKind::Still | SourceKind::ImageSequence => return Err(not(MediaError::Unsupported("stills are not opened as clips".into()))),
    };
    Ok(Clip { info, route })
}

/// The movie's DXV track with its first packet, when that packet's header is valid.
pub(crate) fn first_dxv_packet(path: &Path) -> Result<(Movie, Vec<u8>), MediaError> {
    let movie = Movie::open(path)?;
    let track = movie.track(Codec::Dxv).ok_or(MediaError::NoVideoTrack)?;
    let mut packet = Vec::new();
    movie.read_sample(track, 0, &mut packet)?;
    dxvkit_plus_core::dxv::parse_header(&packet).map_err(|e| MediaError::Dxv(e.to_string()))?;
    Ok((movie, packet))
}
