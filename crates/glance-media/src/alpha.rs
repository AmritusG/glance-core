//! What a clip's alpha means: the Alpha Type rule, the "looks premultiplied / looks straight" hint
//! and the key a clip's choice is remembered by — Swift Glance's `AlphaType.swift`
//! (glance/Sources/GlanceCore/AlphaType.swift), number for number and byte for byte, so a choice
//! made in the Swift apps still applies to the same clip in the Rust ones.

use crate::route::{Clip, Route, first_dxv_packet};
use crate::{MediaInfo, SourceKind};
use dxvkit_plus_core::container::Codec;
use sha2::{Digest, Sha256};
use std::path::Path;

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

impl AlphaView {
    /// The word Swift Glance stores in `Glance.alphaTypeByClip` (`AlphaType.rawValue`).
    pub fn stored_name(self) -> &'static str {
        match self {
            AlphaView::Premultiplied => "premultiplied",
            AlphaView::Straight => "straight",
        }
    }
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

/// The hint's reading of a clip's colour (`AlphaLook`). It never changes the setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AlphaLook {
    LooksPremultiplied,
    LooksStraight,
    CantTell,
}

impl AlphaLook {
    /// The words shown beside the control.
    pub fn label(self) -> &'static str {
        match self {
            AlphaLook::LooksPremultiplied => "looks premultiplied",
            AlphaLook::LooksStraight => "looks straight",
            AlphaLook::CantTell => "can't tell",
        }
    }
}

/// Fewer semi-transparent samples than this cannot be classified.
pub const LOOK_MINIMUM_SAMPLES: u64 = 500;
/// At or below this share of bright samples: looks premultiplied (0.5 %).
pub const LOOK_PREMULTIPLIED_AT_MOST: f64 = 0.005;
/// At or above this share: looks straight (3 %).
pub const LOOK_STRAIGHT_AT_LEAST: f64 = 0.03;

/// The hint's counts over one RGBA8 frame as decoded (`AlphaLookMetric.count`): every 7th pixel in
/// reading order whose alpha is above 8 and below 200 is a sample; a sample whose largest colour
/// channel is above its alpha + 24 is "exceeding" (premultiplied colour cannot exceed its alpha).
pub fn alpha_look_count(rgba: &[u8]) -> (u64, u64) {
    let (mut samples, mut exceeding) = (0u64, 0u64);
    for [r, g, b, a] in rgba.as_chunks::<4>().0.iter().step_by(7) {
        if *a > 8 && *a < 200 {
            samples = samples.saturating_add(1);
            if u16::from(*r.max(g).max(b)) > u16::from(*a).saturating_add(24) {
                exceeding = exceeding.saturating_add(1);
            }
        }
    }
    (samples, exceeding)
}

/// The hint from the counts (`AlphaLookMetric.classify`).
pub fn alpha_look(samples: u64, exceeding: u64) -> AlphaLook {
    if samples < LOOK_MINIMUM_SAMPLES {
        return AlphaLook::CantTell;
    }
    let share = exceeding as f64 / samples as f64;
    if share <= LOOK_PREMULTIPLIED_AT_MOST {
        AlphaLook::LooksPremultiplied
    } else if share >= LOOK_STRAIGHT_AT_LEAST {
        AlphaLook::LooksStraight
    } else {
        AlphaLook::CantTell
    }
}

/// What the Alpha Type control shows for a clip (`AlphaTypeReport`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlphaReport {
    pub rule: AlphaRule,
    /// The hint, for clips whose Alpha Type is the user's choice (DXV with alpha).
    pub look: Option<AlphaLook>,
    /// Semi-transparent samples over the first and the middle frame, and how many were brighter
    /// than their alpha + 24.
    pub samples: u64,
    pub exceeding: u64,
}

impl AlphaReport {
    fn plain(rule: AlphaRule) -> AlphaReport {
        AlphaReport { rule, look: None, samples: 0, exceeding: 0 }
    }

    /// The share of samples brighter than their alpha + 24 (0 without samples).
    pub fn share(&self) -> f64 {
        if self.samples == 0 { 0.0 } else { self.exceeding as f64 / self.samples as f64 }
    }
}

/// The Alpha Type report of a routed clip (`AlphaTypeProbe.probe`). DXV with alpha: the user's
/// choice, with the hint measured on the first and the middle frame (frame `count / 2`), decoded
/// as stored. A DXV clip whose probe frames do not decode reports [`AlphaRule::Opaque`], as Swift
/// Glance's probe does (no control). Everything else: [`alpha_rule`], no hint.
pub fn alpha_report(clip: &Clip) -> AlphaReport {
    let rule = alpha_rule(&clip.info);
    if clip.route != Route::Dxv || rule != AlphaRule::Choosable {
        return AlphaReport::plain(rule);
    }
    let measure = || -> Option<(u64, u64)> {
        let mut source = clip.open().ok()?;
        let middle = clip.info.frames / 2;
        let mut frames = vec![0, middle];
        frames.dedup();
        let (mut samples, mut exceeding) = (0u64, 0u64);
        for frame in frames {
            let (s, e) = alpha_look_count(&source.decode(frame).ok()?.to_rgba().ok()?);
            samples = samples.saturating_add(s);
            exceeding = exceeding.saturating_add(e);
        }
        Some((samples, exceeding))
    };
    match measure() {
        Some((samples, exceeding)) => AlphaReport { rule, look: Some(alpha_look(samples, exceeding)), samples, exceeding },
        None => AlphaReport::plain(AlphaRule::Opaque),
    }
}

/// The key a DXV clip's Alpha Type is remembered by (`AlphaTypeProbe.clipKey`): the lower-case hex
/// SHA-256 of
///
/// 1. the ASCII text `"<file size in bytes> <frame count> <width>x<height>"` (decimal, one space
///    between the first three, a lower-case `x`),
/// 2. the first DXV packet, whole (the bytes of sample 0, header included),
/// 3. the last DXV packet, whole (sample `count - 1`; for a one-frame clip the same packet again),
///
/// fed in that order with nothing between them. Width and height are the DXV track's sample-entry
/// size. `None` for a file that is not a DXV movie whose first packet states a variant — the files
/// Swift Glance's `DXVMovie` refuses. A renamed or moved copy keeps its key; other content at the
/// same path gets another.
pub fn clip_key(path: &Path) -> Option<String> {
    let (movie, first) = first_dxv_packet(path).ok()?;
    let track = movie.track(Codec::Dxv)?;
    let mut last = Vec::new();
    movie.read_sample(track, track.frame_count().checked_sub(1)?, &mut last).ok()?;
    // Swift reads the size separately and hashes 0 when that fails.
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let mut hash = Sha256::new();
    hash.update(format!("{bytes} {} {}x{}", track.frame_count(), track.width, track.height).as_bytes());
    hash.update(&first);
    hash.update(&last);
    Some(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
