//! # glance-media — the Glance family's one way to open a media file
//!
//! A thin facade over DXVKit Plus (`dxvkit-plus-core`, and `dxvkit-plus-apple` on macOS): probe a
//! path, open a frame-exact decoder for it, and name its codec. Demux, HAP, DXV and the native OS
//! codecs are DXVKit Plus's; nothing is decoded here, so there is one decoder per codec across the
//! whole programme (docs/00-ROADMAP.md §2).
//!
//! What this crate adds is the family's policy:
//! - the platform backend is chosen here, once (macOS: AVFoundation / VideoToolbox; Windows: none
//!   until DXVKit Plus ships its Media Foundation backend — native codecs then report
//!   `Unsupported`, never a silent black frame);
//! - frames always come out **as stored** (`AlphaMode::Straight` in DXVKit Plus's terms). Whether
//!   the colour is read as premultiplied or straight is a viewing decision ([`AlphaView`]) that the
//!   present pass in `glance-gpu` applies, so the per-clip switch never re-decodes a frame;
//! - which clips get that switch ([`alpha_rule`]), as Glance 0.18 has it: DXV alpha is the user's
//!   per-clip choice, HAP alpha is straight, opaque clips have none.
#![forbid(unsafe_code)]

use dxvkit_plus_core::source::{self, AlphaMode, ImportOptions};
use std::path::Path;

pub use dxvkit_plus_core::MediaError;
pub use dxvkit_plus_core::backend::NativeFrame;
pub use dxvkit_plus_core::cache::{self, CacheSettings, CacheStats, Player};
pub use dxvkit_plus_core::source::{Decoded, FrameSource, MediaInfo, PixelFormat, Plane, Shading, SourceKind};
pub use dxvkit_plus_core::time::{FrameRate, TICKS_PER_SECOND, Tick};
pub use dxvkit_plus_core::transport;

/// How a viewer reads a clip's colour against its alpha: the family's "Alpha Type" (Resolume
/// Arena's name and default).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AlphaView {
    /// Colour already multiplied by alpha: blended as stored.
    #[default]
    Premultiplied,
    /// Colour stored straight: multiplied by its alpha before the blend.
    Straight,
}

/// What a clip's file says about its alpha, which decides whether the viewer offers the switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaRule {
    /// No alpha: nothing to choose.
    Opaque,
    /// The format fixes it (HAP alpha is straight — Glance 0.18, Amrit 2026-10-10).
    Stated(AlphaView),
    /// The file does not say (DXV, DXVKit spec 8.5): the user's per-clip choice, default
    /// [`AlphaView::Premultiplied`]. Native codecs with alpha land here too until DXVKit Plus
    /// reports the format description's alpha mode (Glance reads it as stated).
    Choosable,
}

/// The alpha rule of a probed clip.
pub fn alpha_rule(info: &MediaInfo) -> AlphaRule {
    match (info.has_alpha, info.kind) {
        (false, _) => AlphaRule::Opaque,
        (true, SourceKind::Hap) => AlphaRule::Stated(AlphaView::Straight),
        (true, _) => AlphaRule::Choosable,
    }
}

#[cfg(target_os = "macos")]
fn platform_backend() -> Option<std::sync::Arc<dyn dxvkit_plus_core::backend::Backend>> {
    Some(std::sync::Arc::new(dxvkit_plus_apple::AppleBackend))
}

#[cfg(not(target_os = "macos"))]
fn platform_backend() -> Option<std::sync::Arc<dyn dxvkit_plus_core::backend::Backend>> {
    None
}

fn import_options() -> ImportOptions {
    ImportOptions { backend: platform_backend(), alpha_mode: AlphaMode::Straight, ..Default::default() }
}

/// What the file is, without decoding a pixel: codec, size, frame count, exact rate, timecode.
pub fn probe(path: &Path) -> Result<MediaInfo, MediaError> {
    let p = path.to_str().ok_or_else(|| MediaError::Invalid(format!("{} is not a UTF-8 path", path.display())))?;
    source::probe(p, &import_options())
}

/// A frame-exact decoder for a probed file. A frame's chunks decompress on worker threads.
pub fn open(info: &MediaInfo) -> Result<Box<dyn FrameSource>, MediaError> {
    source::open(info, &import_options(), true)
}

/// The codec's name as the apps show it. HAP is named by its FourCC; a DXV track's FourCC does not
/// say which variant it holds (the packet does), so DXV is named from a decoded frame's planes by
/// [`dxv_variant_label`].
pub fn codec_label(info: &MediaInfo) -> String {
    match (info.kind, info.codec.as_str()) {
        (SourceKind::Hap, "Hap1") => "HAP".into(),
        (SourceKind::Hap, "Hap5") => "HAP Alpha".into(),
        (SourceKind::Hap, "HapY") => "HAP Q".into(),
        (SourceKind::Hap, "HapM") => "HAP Q Alpha".into(),
        (SourceKind::Hap, "HapA") => "HAP Alpha-Only".into(),
        (SourceKind::Hap, "Hap7") => "HAP R".into(),
        (SourceKind::Hap, "HapH") => "HAP HDR".into(),
        (SourceKind::Dxv, _) => "DXV".into(),
        (SourceKind::Lmbd, _) => "LMBD".into(),
        (_, codec) => codec.to_string(),
    }
}

/// The DXV variant a decoded DXV frame holds, from its planes (BC1 = Normal Quality, BC3 = Normal
/// Quality With Alpha, BC4 + BC5 = High Quality, BC5 + BC5 = High Quality With Alpha). `None` for a
/// frame that is not one of those shapes.
pub fn dxv_variant_label(frame: &Decoded) -> Option<&'static str> {
    let Decoded::Planes { planes, .. } = frame else { return None };
    match planes.iter().map(|p| p.format).collect::<Vec<_>>().as_slice() {
        [PixelFormat::Bc1] => Some("Normal Quality"),
        [PixelFormat::Bc3] => Some("Normal Quality With Alpha"),
        [PixelFormat::Bc4, PixelFormat::Bc5] => Some("High Quality"),
        [PixelFormat::Bc5, PixelFormat::Bc5] => Some("High Quality With Alpha"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(kind: SourceKind, codec: &str) -> MediaInfo {
        MediaInfo { path: String::new(), kind, codec: codec.into(), width: 4, height: 4, frames: 1, rate: FrameRate::FPS_30, has_alpha: false, timecode: None }
    }

    fn frame(formats: &[PixelFormat]) -> Decoded {
        let planes = formats.iter().map(|&format| Plane { format, width: 4, height: 4, data: Vec::new() }).collect();
        Decoded::Planes { index: 0, width: 4, height: 4, shading: Shading::Rgba { opaque: false }, planes }
    }

    #[test]
    fn dxv_alpha_is_the_users_choice_and_hap_alpha_is_straight() {
        assert_eq!(AlphaView::default(), AlphaView::Premultiplied);
        let with_alpha = |kind, codec| MediaInfo { has_alpha: true, ..info(kind, codec) };
        assert_eq!(alpha_rule(&with_alpha(SourceKind::Dxv, "DXD3")), AlphaRule::Choosable);
        assert_eq!(alpha_rule(&with_alpha(SourceKind::Hap, "HapM")), AlphaRule::Stated(AlphaView::Straight));
        assert_eq!(alpha_rule(&with_alpha(SourceKind::Hap, "Hap5")), AlphaRule::Stated(AlphaView::Straight));
        assert_eq!(alpha_rule(&info(SourceKind::Dxv, "DXD3")), AlphaRule::Opaque);
    }

    #[test]
    fn hap_is_named_by_fourcc_and_unknown_codecs_keep_theirs() {
        assert_eq!(codec_label(&info(SourceKind::Hap, "HapM")), "HAP Q Alpha");
        assert_eq!(codec_label(&info(SourceKind::Hap, "Hap1")), "HAP");
        assert_eq!(codec_label(&info(SourceKind::Native, "ap4h")), "ap4h");
        // A DXV FourCC never names the variant.
        assert_eq!(codec_label(&info(SourceKind::Dxv, "DXD3")), "DXV");
    }

    #[test]
    fn a_dxv_variant_is_read_from_the_planes() {
        assert_eq!(dxv_variant_label(&frame(&[PixelFormat::Bc1])), Some("Normal Quality"));
        assert_eq!(dxv_variant_label(&frame(&[PixelFormat::Bc5, PixelFormat::Bc5])), Some("High Quality With Alpha"));
        assert_eq!(dxv_variant_label(&frame(&[PixelFormat::Bc7])), None);
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        assert!(probe(Path::new("/nonexistent/clip.mov")).is_err());
    }
}
