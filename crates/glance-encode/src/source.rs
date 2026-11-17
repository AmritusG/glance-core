//! Source readers: a movie in, straight-or-tagged RGBA frames out, sequentially (Swift GlEnc 1.6.2
//! `SourceFrameReader.swift`, `HAPSourceReader.swift`). Routing is by the first video track's sample-entry FourCC
//! (`SourceFrameReader.swift:140-164`): DXV FourCCs → the DXV reader, `Hap1` / `Hap5` / `HapY` / `HapM` → the HAP
//! reader, everything else → the platform's decoder. A source's geometry and rate are validated at the reader
//! boundary with Swift's limits and words.

use crate::EncodeError;
use crate::frame::{Frame, FrameAlpha, Pts};
use dxvkit_plus_core::container::{Movie, VideoTrack};
use std::fmt;
use std::path::Path;

/// The sample-entry FourCCs the DXV reader takes (`SourceFrameReader.swift:140`; the literal `DXV3` is not one).
pub const DXV_SOURCE_FOURCCS: [&[u8; 4]; 7] = [b"DXT1", b"DXT5", b"YCG6", b"YG10", b"DXDI", b"DXD3", b"DXD4"];
/// The HAP FourCCs read as sources (HapA, an alpha-only image, is not a source).
pub const HAP_SOURCE_FOURCCS: [&[u8; 4]; 4] = [b"Hap1", b"Hap5", b"HapY", b"HapM"];

/// Swift's `SourceGeometryLimits`.
pub const MIN_DIMENSION: u32 = 1;
pub const MAX_DIMENSION: u32 = 65_535;
pub const MAX_FRAME_PIXELS: u64 = 16_384 * 16_384;

/// The DXV source's colour convention: the user's "Alpha Type" (DXV files do not say). Default Premultiplied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AlphaType {
    #[default]
    Premultiplied,
    Straight,
}

impl AlphaType {
    pub fn raw(self) -> &'static str {
        match self {
            AlphaType::Premultiplied => "premultiplied",
            AlphaType::Straight => "straight",
        }
    }
    pub fn from_raw(s: &str) -> Option<AlphaType> {
        match s {
            "premultiplied" => Some(AlphaType::Premultiplied),
            "straight" => Some(AlphaType::Straight),
            _ => None,
        }
    }
}

/// Which reader a source goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceKind {
    Dxv,
    Hap,
    Native,
}

/// Swift's `SourceReaderError`, with its descriptions (`SourceFrameReader.swift:18-63`). Every one of them makes a
/// queued row "Unusable source: …" at add time (`EncodeQueue.swift:429-436`).
#[derive(Clone, Debug, PartialEq)]
pub enum SourceError {
    NoVideoTrack,
    ReaderStartFailed(String),
    ReaderFailed(String),
    DxvDemuxFailed(String),
    DxvDecodeFailed { index: u64, why: String },
    DimensionsInvalid { width: i64, height: i64 },
    DimensionsTooLarge { width: i64, height: i64 },
    FrameTooLarge { width: i64, height: i64 },
    FrameRateInvalid(f64),
    DurationInvalid(f64),
}

/// A Swift `Double`'s description: `30.0`, `29.97002997002997`, `nan`, `inf`.
pub fn swift_double(v: f64) -> String {
    if v.is_nan() {
        "nan".into()
    } else if v.is_infinite() {
        if v > 0.0 { "inf".into() } else { "-inf".into() }
    } else if v.fract() == 0.0 && v.abs() < 1e16 {
        // Swift prints whole values below 2^53-ish with one decimal ("30.0"); 1e16 and above in exponent form.
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::NoVideoTrack => f.write_str("source has no video track"),
            SourceError::ReaderStartFailed(e) => write!(f, "AVAssetReader.startReading failed: {e}"),
            SourceError::ReaderFailed(e) => write!(f, "AVAssetReader failed mid-stream: {e}"),
            SourceError::DxvDemuxFailed(e) => write!(f, "DXV file could not be read: {e}"),
            SourceError::DxvDecodeFailed { index, why } => write!(f, "DXV frame {index} decode failed: {why}"),
            SourceError::DimensionsInvalid { width, height } => {
                write!(f, "source reports unusable dimensions {width}×{height} — width and height must each be positive")
            }
            SourceError::DimensionsTooLarge { width, height } => {
                write!(f, "source dimensions {width}×{height} exceed the maximum encodable size of {MAX_DIMENSION} pixels per side")
            }
            SourceError::FrameTooLarge { width, height } => {
                write!(f, "source frame {width}×{height} is too large to encode safely (over {MAX_FRAME_PIXELS} pixels)")
            }
            SourceError::FrameRateInvalid(fps) => write!(f, "source reports an invalid frame rate ({} fps)", swift_double(*fps)),
            SourceError::DurationInvalid(d) => write!(f, "source reports an invalid duration ({} seconds)", swift_double(*d)),
        }
    }
}

impl std::error::Error for SourceError {}

/// Swift's `validateSourceGeometry` (`SourceFrameReader.swift:110-132`), in its order.
pub fn validate_geometry(width: i64, height: i64, fps: f64, duration: f64) -> Result<(), SourceError> {
    if width < i64::from(MIN_DIMENSION) || height < i64::from(MIN_DIMENSION) {
        return Err(SourceError::DimensionsInvalid { width, height });
    }
    if width > i64::from(MAX_DIMENSION) || height > i64::from(MAX_DIMENSION) {
        return Err(SourceError::DimensionsTooLarge { width, height });
    }
    if width.saturating_mul(height) > MAX_FRAME_PIXELS as i64 {
        return Err(SourceError::FrameTooLarge { width, height });
    }
    if !fps.is_finite() || fps <= 0.0 {
        return Err(SourceError::FrameRateInvalid(fps));
    }
    if !duration.is_finite() || duration < 0.0 {
        return Err(SourceError::DurationInvalid(duration));
    }
    Ok(())
}

/// What a reader knows before the first frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceInfo {
    pub kind: SourceKind,
    pub fourcc: [u8; 4],
    pub width: u32,
    pub height: u32,
    /// Swift's `sourceFPS` (a `Double` the writer maps back to its exact timescale).
    pub fps: f64,
    /// Swift's `totalFrameCount`: DXV / HAP the sample count, native `round(duration × fps)`.
    pub total_frames: u64,
    /// The DXV reader un-premultiplies (Alpha Type = Premultiplied on a DXV file with alpha), or the native reader
    /// does (a format that states premultiplied alpha, when the output keeps alpha).
    pub unpremultiplies_on_read: bool,
}

/// A sequential frame reader.
pub trait SourceReader: Send {
    fn info(&self) -> SourceInfo;
    /// The next frame in file order; `None` at the end.
    fn next_frame(&mut self) -> Result<Option<Frame>, SourceError>;
}

/// Where a source goes, by its first video track's FourCC.
pub fn source_kind(path: &Path) -> Option<([u8; 4], SourceKind)> {
    let cc = dxvkit_plus_core::container::first_video_fourcc(path)?;
    let kind = if DXV_SOURCE_FOURCCS.iter().any(|c| **c == cc) {
        SourceKind::Dxv
    } else if HAP_SOURCE_FOURCCS.iter().any(|c| **c == cc) {
        SourceKind::Hap
    } else {
        SourceKind::Native
    };
    Some((cc, kind))
}

/// Swift's `makeSourceReader` (`SourceFrameReader.swift:146-165`). `alpha_tag` is the tag the output codec asks
/// native frames to carry (`.last` = [`FrameAlpha::Straight`] for alpha outputs, else [`FrameAlpha::Opaque`]); DXV
/// and HAP frames are always tagged straight (`.last`).
pub fn open_source(path: &Path, alpha_tag: FrameAlpha, alpha_type: AlphaType) -> Result<Box<dyn SourceReader>, SourceError> {
    match source_kind(path) {
        Some((cc, SourceKind::Dxv)) => Ok(Box::new(DxvReader::open(path, cc, alpha_type)?)),
        Some((cc, SourceKind::Hap)) => Ok(Box::new(HapReader::open(path, cc)?)),
        _ => crate::platform::open_native(path, alpha_tag),
    }
}

/// The first video track of a DXV / HAP movie, as the Swift demuxers see it.
fn first_track(path: &Path) -> Result<(Movie, VideoTrack), SourceError> {
    let movie = Movie::open(path).map_err(|e| SourceError::DxvDemuxFailed(e.to_string()))?;
    let track = movie.first().cloned().ok_or(SourceError::NoVideoTrack)?;
    Ok((movie, track))
}

/// The DXV reader (`DxvSourceReader`): DXVKit's index and decode, `rgba()` as stored, un-premultiplied on read when
/// the Alpha Type is Premultiplied and the variant has alpha. Rate = frames / (mdhd duration / timescale) — Glance
/// 0.18.1's `DXVMovieIndex.frameRate` over DXVKit's `mdhd` duration; times `i × first stts delta / timescale`.
pub struct DxvReader {
    info: SourceInfo,
    movie: Movie,
    track: VideoTrack,
    step: f64,
    next: u32,
    buf: Vec<u8>,
}

impl DxvReader {
    pub fn open(path: &Path, fourcc: [u8; 4], alpha_type: AlphaType) -> Result<DxvReader, SourceError> {
        let (movie, track) = first_track(path)?;
        let ts = f64::from(track.timescale);
        let duration = if track.timescale > 0 { track.duration as f64 / ts } else { 0.0 };
        let frames = track.samples.len() as f64;
        let fps = if duration > 0.0 { frames / duration } else { 30.0 };
        let duration_sec = if fps > 0.0 { frames / fps } else { 0.0 };
        validate_geometry(i64::from(track.width), i64::from(track.height), fps, duration_sec)?;
        let step = match track.stts.first() {
            Some(&(_, delta)) if track.timescale > 0 => f64::from(delta) / ts,
            _ => 0.0,
        };
        // DXVKit's `hasAlpha`: the variant of the first packet.
        let mut buf = Vec::new();
        movie.read_sample(&track, 0, &mut buf).map_err(|e| SourceError::DxvDemuxFailed(e.to_string()))?;
        let has_alpha = dxvkit_plus_core::dxv::parse_header(&buf).map(|h| h.variant.has_alpha()).map_err(|e| SourceError::DxvDemuxFailed(e.to_string()))?;
        let info = SourceInfo {
            kind: SourceKind::Dxv,
            fourcc,
            width: track.width,
            height: track.height,
            fps,
            total_frames: track.samples.len() as u64,
            unpremultiplies_on_read: alpha_type == AlphaType::Premultiplied && has_alpha,
        };
        Ok(DxvReader { info, movie, track, step, next: 0, buf })
    }
}

impl SourceReader for DxvReader {
    fn info(&self) -> SourceInfo {
        self.info
    }
    fn next_frame(&mut self) -> Result<Option<Frame>, SourceError> {
        let i = self.next;
        if u64::from(i) >= self.info.total_frames {
            return Ok(None);
        }
        self.next = i.saturating_add(1);
        let fail = |why: String| SourceError::DxvDecodeFailed { index: u64::from(i), why };
        self.movie.read_sample(&self.track, i, &mut self.buf).map_err(|e| fail(e.to_string()))?;
        let blocks = dxvkit_plus_core::dxv::decode_blocks(&self.buf, self.info.width, self.info.height, true).map_err(|e| fail(e.to_string()))?;
        let mut rgba = dxvkit_plus_core::dxv::rgba::blocks_to_rgba(&blocks, false).map_err(|e| fail(e.to_string()))?;
        if self.info.unpremultiplies_on_read {
            dxvkit_plus_enc::alpha::unpremultiply_in_place(&mut rgba);
        }
        let pts = Pts::from_seconds_600(f64::from(i) * self.step);
        Ok(Some(Frame { width: self.info.width, height: self.info.height, rgba, alpha: FrameAlpha::Straight, pts, attachments: None }))
    }
}

/// The HAP reader (`HAPSourceReader`): every sample decoded on the CPU to straight RGBA. Rate = frames / (sum of
/// the sample deltas / timescale) and times = the running sum of deltas (Glance 0.18.1 `HAPDemuxer`).
pub struct HapReader {
    info: SourceInfo,
    movie: Movie,
    track: VideoTrack,
    starts: Vec<f64>,
    next: u32,
    buf: Vec<u8>,
}

impl HapReader {
    pub fn open(path: &Path, fourcc: [u8; 4]) -> Result<HapReader, SourceError> {
        let (movie, track) = first_track(path)?;
        let n = track.samples.len();
        let ts = f64::from(track.timescale.max(1));
        let mut deltas: Vec<u32> = track.stts.iter().flat_map(|&(c, d)| std::iter::repeat_n(d, c as usize)).collect();
        let last = deltas.last().copied().unwrap_or(1);
        deltas.resize(n.max(deltas.len()), last);
        let mut starts = Vec::with_capacity(n);
        let mut ticks: u64 = 0;
        for d in deltas.iter().take(n) {
            starts.push(ticks as f64 / ts);
            ticks = ticks.saturating_add(u64::from(*d));
        }
        let duration = match (starts.last(), deltas.get(n.saturating_sub(1))) {
            (Some(s), Some(d)) if n > 0 => s + f64::from(*d) / ts,
            _ => 0.0,
        };
        let fps = if duration > 0.0 { n as f64 / duration } else { 30.0 };
        let duration_sec = if fps > 0.0 { n as f64 / fps } else { 0.0 };
        validate_geometry(i64::from(track.width), i64::from(track.height), fps, duration_sec)?;
        let info =
            SourceInfo { kind: SourceKind::Hap, fourcc, width: track.width, height: track.height, fps, total_frames: n as u64, unpremultiplies_on_read: false };
        Ok(HapReader { info, movie, track, starts, next: 0, buf: Vec::new() })
    }
}

impl SourceReader for HapReader {
    fn info(&self) -> SourceInfo {
        self.info
    }
    fn next_frame(&mut self) -> Result<Option<Frame>, SourceError> {
        let i = self.next;
        if u64::from(i) >= self.info.total_frames {
            return Ok(None);
        }
        self.next = i.saturating_add(1);
        let fail = |why: String| SourceError::DxvDecodeFailed { index: u64::from(i), why };
        self.movie.read_sample(&self.track, i, &mut self.buf).map_err(|e| fail(e.to_string()))?;
        let (w, h) = (self.info.width, self.info.height);
        let frame = dxvkit_plus_core::hap::decode(&self.buf, w, h, true).map_err(|e| fail(e.to_string()))?;
        let rgba = dxvkit_plus_core::hap::to_rgba(&frame, w, h).map_err(|e| fail(e.to_string()))?;
        let pts = Pts::from_seconds_600(self.starts.get(i as usize).copied().unwrap_or(0.0));
        Ok(Some(Frame { width: w, height: h, rgba, alpha: FrameAlpha::Straight, pts, attachments: None }))
    }
}

impl From<SourceError> for EncodeError {
    fn from(e: SourceError) -> EncodeError {
        EncodeError::Source(e)
    }
}
