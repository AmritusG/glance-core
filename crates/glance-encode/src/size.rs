//! Output size, resize quality, aspect mode and the letterbox rectangle (Swift GlEnc 1.6.2 `ResizeTypes.swift`,
//! `LetterboxFit.swift`).

/// How a frame is scaled (Swift's `ResizeQuality`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResizeQuality {
    Auto,
    Nearest,
    Bilinear,
    Lanczos,
}

impl ResizeQuality {
    pub const ALL: [ResizeQuality; 4] = [ResizeQuality::Auto, ResizeQuality::Nearest, ResizeQuality::Bilinear, ResizeQuality::Lanczos];
    pub fn raw(self) -> &'static str {
        match self {
            ResizeQuality::Auto => "auto",
            ResizeQuality::Nearest => "nearest",
            ResizeQuality::Bilinear => "bilinear",
            ResizeQuality::Lanczos => "lanczos",
        }
    }
    pub fn from_raw(s: &str) -> Option<ResizeQuality> {
        ResizeQuality::ALL.into_iter().find(|q| q.raw() == s)
    }
    pub fn label(self) -> &'static str {
        match self {
            ResizeQuality::Auto => "Auto",
            ResizeQuality::Nearest => "Nearest",
            ResizeQuality::Bilinear => "Bilinear",
            ResizeQuality::Lanczos => "Lanczos",
        }
    }
    /// Auto → Lanczos when the output is no larger than the source on both axes, else Bilinear (`:52-82`).
    pub fn resolved(self, src: (u32, u32), out: (u32, u32)) -> ResizeQuality {
        match self {
            ResizeQuality::Auto if out == src => ResizeQuality::Bilinear,
            ResizeQuality::Auto if out.0 <= src.0 && out.1 <= src.1 => ResizeQuality::Lanczos,
            ResizeQuality::Auto => ResizeQuality::Bilinear,
            q => q,
        }
    }
}

/// The 12 fixed sizes (Swift's `StandardResolution`, raw values as persisted).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StandardResolution {
    Hd1280x720,
    Fhd1920x1080,
    Qhd2560x1440,
    Uhd3840x2160,
    Dci2048x1080,
    Dci4096x2160,
    Sq1024,
    Sq1080,
    Sq2048,
    V720x1280,
    V1080x1920,
    V1440x2560,
}

impl StandardResolution {
    pub const ALL: [StandardResolution; 12] = [
        StandardResolution::Hd1280x720,
        StandardResolution::Fhd1920x1080,
        StandardResolution::Qhd2560x1440,
        StandardResolution::Uhd3840x2160,
        StandardResolution::Dci2048x1080,
        StandardResolution::Dci4096x2160,
        StandardResolution::Sq1024,
        StandardResolution::Sq1080,
        StandardResolution::Sq2048,
        StandardResolution::V720x1280,
        StandardResolution::V1080x1920,
        StandardResolution::V1440x2560,
    ];
    pub fn raw(self) -> &'static str {
        match self {
            StandardResolution::Hd1280x720 => "hd_1280_720",
            StandardResolution::Fhd1920x1080 => "fhd_1920_1080",
            StandardResolution::Qhd2560x1440 => "qhd_2560_1440",
            StandardResolution::Uhd3840x2160 => "uhd_3840_2160",
            StandardResolution::Dci2048x1080 => "dci_2048_1080",
            StandardResolution::Dci4096x2160 => "dci_4096_2160",
            StandardResolution::Sq1024 => "sq_1024",
            StandardResolution::Sq1080 => "sq_1080",
            StandardResolution::Sq2048 => "sq_2048",
            StandardResolution::V720x1280 => "v_720_1280",
            StandardResolution::V1080x1920 => "v_1080_1920",
            StandardResolution::V1440x2560 => "v_1440_2560",
        }
    }
    pub fn from_raw(s: &str) -> Option<StandardResolution> {
        StandardResolution::ALL.into_iter().find(|r| r.raw() == s)
    }
    pub fn dimensions(self) -> (u32, u32) {
        match self {
            StandardResolution::Hd1280x720 => (1280, 720),
            StandardResolution::Fhd1920x1080 => (1920, 1080),
            StandardResolution::Qhd2560x1440 => (2560, 1440),
            StandardResolution::Uhd3840x2160 => (3840, 2160),
            StandardResolution::Dci2048x1080 => (2048, 1080),
            StandardResolution::Dci4096x2160 => (4096, 2160),
            StandardResolution::Sq1024 => (1024, 1024),
            StandardResolution::Sq1080 => (1080, 1080),
            StandardResolution::Sq2048 => (2048, 2048),
            StandardResolution::V720x1280 => (720, 1280),
            StandardResolution::V1080x1920 => (1080, 1920),
            StandardResolution::V1440x2560 => (1440, 2560),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            StandardResolution::Hd1280x720 => "HD 720p — 1280×720",
            StandardResolution::Fhd1920x1080 => "Full HD — 1920×1080",
            StandardResolution::Qhd2560x1440 => "QHD — 2560×1440",
            StandardResolution::Uhd3840x2160 => "UHD 4K — 3840×2160",
            StandardResolution::Dci2048x1080 => "DCI 2K — 2048×1080",
            StandardResolution::Dci4096x2160 => "DCI 4K — 4096×2160",
            StandardResolution::Sq1024 => "Square — 1024×1024",
            StandardResolution::Sq1080 => "Square — 1080×1080",
            StandardResolution::Sq2048 => "Square — 2048×2048",
            StandardResolution::V720x1280 => "Vertical — 720×1280",
            StandardResolution::V1080x1920 => "Vertical — 1080×1920",
            StandardResolution::V1440x2560 => "Vertical — 1440×2560",
        }
    }
}

/// Letterbox (default) or distort (Swift's `AspectMode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AspectMode {
    Letterbox,
    DistortToFill,
}

impl AspectMode {
    pub fn raw(self) -> &'static str {
        match self {
            AspectMode::Letterbox => "letterbox",
            AspectMode::DistortToFill => "distortToFill",
        }
    }
    pub fn from_raw(s: &str) -> Option<AspectMode> {
        match s {
            "letterbox" => Some(AspectMode::Letterbox),
            "distortToFill" => Some(AspectMode::DistortToFill),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            AspectMode::Letterbox => "Fit (letterbox)",
            AspectMode::DistortToFill => "Distort to fill",
        }
    }
}

/// The encoded size (Swift's `OutputSize`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputSize {
    /// The size actually read: the crop size when cropped, else the source size.
    Original,
    Preset(StandardResolution),
    Custom {
        width: u32,
        height: u32,
    },
}

impl OutputSize {
    pub fn resolved(self, source: (u32, u32)) -> (u32, u32) {
        match self {
            OutputSize::Original => source,
            OutputSize::Preset(p) => p.dimensions(),
            OutputSize::Custom { width, height } => (width, height),
        }
    }
}

/// The picture's place on a letterboxed canvas (Swift's `LetterboxRect`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LetterboxRect {
    pub inset_x: u32,
    pub inset_y: u32,
    pub width: u32,
    pub height: u32,
}

impl LetterboxRect {
    pub fn fills(&self, canvas: (u32, u32)) -> bool {
        self.inset_x == 0 && self.inset_y == 0 && (self.width, self.height) == canvas
    }
}

/// Swift's `roundToFourMultiple`: `((n + 2) / 4) * 4`, at least 4.
fn round_to_four(n: u64) -> u64 {
    if n == 0 {
        return 4;
    }
    (n.saturating_add(2) / 4).saturating_mul(4).max(4)
}

/// Swift's `letterboxRect` (`LetterboxFit.swift:107-146`): cross-multiplied aspect comparison; the inner side
/// rounded to a multiple of 4 (at least 4) and clamped to the target; centred with integer halves.
pub fn letterbox_rect(src: (u32, u32), dst: (u32, u32)) -> LetterboxRect {
    let (sw, sh, dw, dh) = (u64::from(src.0), u64::from(src.1), u64::from(dst.0), u64::from(dst.1));
    let (a, b) = (sw.saturating_mul(dh), sh.saturating_mul(dw));
    if a == b || sw == 0 || sh == 0 {
        return LetterboxRect { inset_x: 0, inset_y: 0, width: dst.0, height: dst.1 };
    }
    let (iw, ih) = if a > b { (dw, round_to_four(sh.saturating_mul(dw) / sw).min(dh)) } else { (round_to_four(sw.saturating_mul(dh) / sh).min(dw), dh) };
    let (iw, ih) = (u32::try_from(iw).unwrap_or(dst.0), u32::try_from(ih).unwrap_or(dst.1));
    LetterboxRect { inset_x: dst.0.saturating_sub(iw) / 2, inset_y: dst.1.saturating_sub(ih) / 2, width: iw, height: ih }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterbox_matches_swifts_cases() {
        // 16:9 into a square: pillar? no — source wider → full width, height rounded to 4.
        assert_eq!(letterbox_rect((1920, 1080), (1080, 1080)), LetterboxRect { inset_x: 0, inset_y: 236, width: 1080, height: 608 });
        // 9:16 into 16:9: full height, width rounded.
        assert_eq!(letterbox_rect((1080, 1920), (1920, 1080)), LetterboxRect { inset_x: 656, inset_y: 0, width: 608, height: 1080 });
        // Equal aspect: no bars.
        assert!(letterbox_rect((1280, 720), (1920, 1080)).fills((1920, 1080)));
        // Tiny: at least 4.
        assert_eq!(letterbox_rect((1000, 1), (8, 8)).height, 4);
    }

    #[test]
    fn auto_quality_is_direction_based() {
        assert_eq!(ResizeQuality::Auto.resolved((1920, 1080), (1280, 720)), ResizeQuality::Lanczos);
        assert_eq!(ResizeQuality::Auto.resolved((1280, 720), (1920, 1080)), ResizeQuality::Bilinear);
        assert_eq!(ResizeQuality::Auto.resolved((1920, 1080), (2000, 1000)), ResizeQuality::Bilinear);
        assert_eq!(ResizeQuality::Nearest.resolved((1, 1), (2, 2)), ResizeQuality::Nearest);
    }
}
