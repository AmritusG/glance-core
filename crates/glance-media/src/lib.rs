//! # glance-media — the Glance family's one way to open a media file
//!
//! A thin facade over DXVKit Plus (`dxvkit-plus-core`, and `dxvkit-plus-apple` on macOS): route a
//! path by its content ([`route`]), open a frame-exact decoder for it ([`Clip::open`]), and name its
//! codec. Demux, HAP, DXV and the native OS
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
//!   per-clip choice, HAP alpha is straight, opaque clips have none; the hint beside it
//!   ([`alpha_report`]) and the key the choice is remembered by ([`clip_key`]), both identical to
//!   Swift Glance's;
//! - what the viewer says about a file nothing plays ([`NotPlayable`]), in Swift Glance's words.
#![forbid(unsafe_code)]

mod alpha;
mod route;

use dxvkit_plus_core::source::{AlphaMode, ImportOptions};

pub use alpha::{
    AlphaLook, AlphaReport, AlphaRule, AlphaView, LOOK_MINIMUM_SAMPLES, LOOK_PREMULTIPLIED_AT_MOST, LOOK_STRAIGHT_AT_LEAST, alpha_look, alpha_look_count,
    alpha_report, alpha_rule, clip_key,
};
pub use route::{Clip, NotPlayable, Route, route};

pub use dxvkit_plus_core::container::first_video_fourcc;
pub use dxvkit_plus_core::dxv::Variant as DxvVariant;
pub use dxvkit_plus_core::names::{self, unsupported_message};

pub use dxvkit_plus_core::MediaError;
pub use dxvkit_plus_core::backend::NativeFrame;
pub use dxvkit_plus_core::bc;
pub use dxvkit_plus_core::cache::{self, CacheSettings, CacheStats, Player};
pub use dxvkit_plus_core::source::{Decoded, FrameSource, MediaInfo, PixelFormat, Plane, Shading, SourceKind};
pub use dxvkit_plus_core::time::{FrameRate, TICKS_PER_SECOND, Tick};
pub use dxvkit_plus_core::transport;

#[cfg(target_os = "macos")]
fn platform_backend() -> Option<std::sync::Arc<dyn dxvkit_plus_core::backend::Backend>> {
    Some(std::sync::Arc::new(dxvkit_plus_apple::AppleBackend))
}

#[cfg(not(target_os = "macos"))]
fn platform_backend() -> Option<std::sync::Arc<dyn dxvkit_plus_core::backend::Backend>> {
    None
}

pub(crate) fn import_options() -> ImportOptions {
    ImportOptions { backend: platform_backend(), alpha_mode: AlphaMode::Straight, ..Default::default() }
}

/// The DXV variant's name as the apps show it (Arena's names; the text is glance-core's, the variant
/// is `MediaInfo::dxv_variant`, read by DXVKit Plus from the first packet). Legacy DXV (DXV1 / 2) is
/// named by the same blocks.
pub fn dxv_variant_label(variant: DxvVariant) -> &'static str {
    match variant {
        DxvVariant::Dxt1 | DxvVariant::LegacyDxt1 => "Normal Quality",
        DxvVariant::Dxt5 | DxvVariant::LegacyDxt5 => "Normal Quality With Alpha",
        DxvVariant::Ycg6 => "High Quality",
        DxvVariant::Yg10 => "High Quality With Alpha",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(kind: SourceKind, codec: &str) -> MediaInfo {
        MediaInfo { kind, codec: codec.into(), width: 4, height: 4, frames: 1, rate: FrameRate::FPS_30, ..MediaInfo::default() }
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
    fn a_dxv_variant_has_arenas_name() {
        assert_eq!(dxv_variant_label(DxvVariant::Dxt1), "Normal Quality");
        assert_eq!(dxv_variant_label(DxvVariant::LegacyDxt5), "Normal Quality With Alpha");
        assert_eq!(dxv_variant_label(DxvVariant::Yg10), "High Quality With Alpha");
    }
}
