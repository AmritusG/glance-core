//! What GlEnc can write: the output codecs, their containers and their per-codec rules (Swift GlEnc 1.6.2
//! `OutputCodec.swift`, `DXVFormat.swift`, `FrameSink.swift:138-189`, `VideoEncodeSettings.swift`).

/// The DXV3 and HAP formats (Swift's `DXVFormat`; raw values are its persisted names).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DxvFormat {
    Dxt1,
    Dxt5,
    Ycg6,
    Yg10,
    Hap1,
    Hap5,
    HapY,
    HapA,
    HapM,
}

impl DxvFormat {
    pub const ALL: [DxvFormat; 9] = [
        DxvFormat::Dxt1,
        DxvFormat::Dxt5,
        DxvFormat::Ycg6,
        DxvFormat::Yg10,
        DxvFormat::Hap1,
        DxvFormat::Hap5,
        DxvFormat::HapY,
        DxvFormat::HapA,
        DxvFormat::HapM,
    ];
    /// Swift's raw value (`DXVFormat.swift:32-41`).
    pub fn raw(self) -> &'static str {
        match self {
            DxvFormat::Dxt1 => "dxt1",
            DxvFormat::Dxt5 => "dxt5",
            DxvFormat::Ycg6 => "ycg6",
            DxvFormat::Yg10 => "yg10",
            DxvFormat::Hap1 => "hap1",
            DxvFormat::Hap5 => "hap5",
            DxvFormat::HapY => "hapy",
            DxvFormat::HapA => "hapa",
            DxvFormat::HapM => "hapm",
        }
    }
    pub fn from_raw(s: &str) -> Option<DxvFormat> {
        DxvFormat::ALL.into_iter().find(|f| f.raw() == s)
    }
    /// Whether the format stores alpha (DXT5, YG10, Hap5, HapA, HapM).
    pub fn has_alpha(self) -> bool {
        matches!(self, DxvFormat::Dxt5 | DxvFormat::Yg10 | DxvFormat::Hap5 | DxvFormat::HapA | DxvFormat::HapM)
    }
    pub fn is_hap(self) -> bool {
        matches!(self, DxvFormat::Hap1 | DxvFormat::Hap5 | DxvFormat::HapY | DxvFormat::HapA | DxvFormat::HapM)
    }
    /// The stream FourCC of the sample entry: `DXD3` for every DXV3 variant, the HAP name otherwise.
    pub fn stream_fourcc(self) -> [u8; 4] {
        match self {
            DxvFormat::Dxt1 | DxvFormat::Dxt5 | DxvFormat::Ycg6 | DxvFormat::Yg10 => *b"DXD3",
            DxvFormat::Hap1 => *b"Hap1",
            DxvFormat::Hap5 => *b"Hap5",
            DxvFormat::HapY => *b"HapY",
            DxvFormat::HapA => *b"HapA",
            DxvFormat::HapM => *b"HapM",
        }
    }
}

/// The ProRes variants GlEnc writes (Swift's `ProResVariant`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProResVariant {
    ProRes422,
    ProRes422Hq,
    ProRes422Lt,
    ProRes422Proxy,
    ProRes4444,
}

impl ProResVariant {
    pub const ALL: [ProResVariant; 5] =
        [ProResVariant::ProRes422, ProResVariant::ProRes422Hq, ProResVariant::ProRes422Lt, ProResVariant::ProRes422Proxy, ProResVariant::ProRes4444];
    /// Swift's raw value (`FrameSink.swift:138-143`).
    pub fn raw(self) -> &'static str {
        match self {
            ProResVariant::ProRes422 => "proRes422",
            ProResVariant::ProRes422Hq => "proRes422HQ",
            ProResVariant::ProRes422Lt => "proRes422LT",
            ProResVariant::ProRes422Proxy => "proRes422Proxy",
            ProResVariant::ProRes4444 => "proRes4444",
        }
    }
    pub fn from_raw(s: &str) -> Option<ProResVariant> {
        ProResVariant::ALL.into_iter().find(|v| v.raw() == s)
    }
    pub fn has_alpha(self) -> bool {
        self == ProResVariant::ProRes4444
    }
    /// The sample-entry FourCC AVFoundation writes.
    pub fn codec_tag(self) -> &'static str {
        match self {
            ProResVariant::ProRes422 => "apcn",
            ProResVariant::ProRes422Hq => "apch",
            ProResVariant::ProRes422Lt => "apcs",
            ProResVariant::ProRes422Proxy => "apco",
            ProResVariant::ProRes4444 => "ap4h",
        }
    }
    /// The menu label (`FrameSink.swift:169-177`).
    pub fn label(self) -> &'static str {
        match self {
            ProResVariant::ProRes422 => "ProRes 422",
            ProResVariant::ProRes422Hq => "ProRes 422 HQ",
            ProResVariant::ProRes422Lt => "ProRes 422 LT",
            ProResVariant::ProRes422Proxy => "ProRes 422 Proxy",
            ProResVariant::ProRes4444 => "ProRes 4444 (alpha)",
        }
    }
    /// The file-name token (`FrameSink.swift:181-189`).
    pub fn name_token(self) -> &'static str {
        match self {
            ProResVariant::ProRes4444 => "ProRes 4444",
            v => v.label(),
        }
    }
}

/// An output codec (Swift's `OutputCodec`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputCodec {
    Dxv(DxvFormat),
    ProRes(ProResVariant),
    H264,
    Hevc,
    Mjpeg,
}

impl OutputCodec {
    /// Crop sizes and resize targets must be multiples of this (`OutputCodec.swift:53-58`): VideoToolbox rounds odd
    /// H.264 / HEVC sizes down silently, so they need even numbers.
    pub fn dimension_alignment(self) -> u32 {
        match self {
            OutputCodec::H264 | OutputCodec::Hevc => 2,
            _ => 1,
        }
    }
    pub fn has_alpha(self) -> bool {
        match self {
            OutputCodec::Dxv(f) => f.has_alpha(),
            OutputCodec::ProRes(v) => v.has_alpha(),
            _ => false,
        }
    }
    /// H.264 / HEVC have the Advanced settings (rate control, keyframes, profile).
    pub fn has_advanced_settings(self) -> bool {
        matches!(self, OutputCodec::H264 | OutputCodec::Hevc)
    }
    pub fn allowed_containers(self) -> &'static [OutputContainer] {
        match self {
            OutputCodec::H264 | OutputCodec::Hevc => &[OutputContainer::Mov, OutputContainer::Mp4],
            _ => &[OutputContainer::Mov],
        }
    }
    pub fn dxv_format(self) -> Option<DxvFormat> {
        if let OutputCodec::Dxv(f) = self { Some(f) } else { None }
    }
}

/// QuickTime or MPEG-4 (Swift's `OutputContainer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputContainer {
    Mov,
    Mp4,
}

impl OutputContainer {
    pub fn raw(self) -> &'static str {
        match self {
            OutputContainer::Mov => "mov",
            OutputContainer::Mp4 => "mp4",
        }
    }
    pub fn from_raw(s: &str) -> Option<OutputContainer> {
        match s {
            "mov" => Some(OutputContainer::Mov),
            "mp4" => Some(OutputContainer::Mp4),
            _ => None,
        }
    }
    pub fn ext(self) -> &'static str {
        self.raw()
    }
    pub fn label(self) -> &'static str {
        match self {
            OutputContainer::Mov => "QuickTime (.mov)",
            OutputContainer::Mp4 => "MPEG-4 (.mp4)",
        }
    }
    /// `.mp4` carries AAC, capped at 48 kHz; `.mov` carries 32-bit PCM at any rate.
    pub fn uses_aac(self) -> bool {
        self == OutputContainer::Mp4
    }
    pub fn max_audio_rate(self) -> u32 {
        if self.uses_aac() { 48_000 } else { u32::MAX }
    }
}

/// H.264 / HEVC rate control (Swift's `VideoEncodeSettings.RateControl`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RateControl {
    /// Constant quality 0…1 (`AVVideoQualityKey`); the UI offers 0.1–1.0.
    Quality(f64),
    /// Average bitrate in Mbps (`AVVideoAverageBitRateKey` = Int(mbps × 1 000 000)).
    Bitrate(f64),
}

/// The H.264 profile (`AVVideoProfileLevelH264*AutoLevel`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum H264Profile {
    Baseline,
    Main,
    High,
}

impl H264Profile {
    pub fn raw(self) -> &'static str {
        match self {
            H264Profile::Baseline => "baseline",
            H264Profile::Main => "main",
            H264Profile::High => "high",
        }
    }
    pub fn from_raw(s: &str) -> Option<H264Profile> {
        [H264Profile::Baseline, H264Profile::Main, H264Profile::High].into_iter().find(|p| p.raw() == s)
    }
}

/// The H.264 / HEVC Advanced settings (Swift's `VideoEncodeSettings`, default quality 0.6, keyframes auto, High).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoSettings {
    pub rate_control: RateControl,
    /// Largest keyframe interval in frames; 0 = the codec's default.
    pub keyframe_interval: u32,
    pub h264_profile: H264Profile,
}

impl Default for VideoSettings {
    fn default() -> VideoSettings {
        VideoSettings { rate_control: RateControl::Quality(0.6), keyframe_interval: 0, h264_profile: H264Profile::High }
    }
}

/// Motion JPEG's fixed quality (`CoreEncoder.swift`, `mjpegDefaultQuality`).
pub const MJPEG_QUALITY: f64 = 0.85;
