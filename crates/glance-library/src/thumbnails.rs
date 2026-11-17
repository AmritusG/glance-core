//! MaCrate's poster thumbnails (CrateKit/ThumbnailService.swift, ThumbnailExtractor.swift, ThumbnailCache.swift,
//! MemoryThumbnailCache.swift, ThumbnailDownscale.swift, ThumbnailPrefetcher.swift; CrateCore/ThumbnailMigration.swift):
//! one picture per clip, chosen by a walk over the first second, baked over a checkerboard, kept as a JPEG in
//! `<caches>/<bundle id>/thumbs/<16 hex>.jpg` (the folder is the caller's; the key is [`crate::thumbkey`]'s, so the
//! JPEGs Swift MaCrate left are used as they are) and in a 1 GiB / 500-entry memory LRU.
//!
//! - **Decoders.** DXV (`DXV3` `DXD3` `DXD4` `DXDI`) and HAP (`Hap1` `Hap5` `HapY` `HapM` `HapA`) through
//!   glance-media (DXVKit Plus): each frame as stored ([`glance_media::Decoded::to_rgba`] — the raw texels Swift's
//!   `CPURender.cgImageFromPacket` / `HAPThumbnail.rgbaOfFrame` give), read as premultiplied or straight by the
//!   clip's Alpha Type for DXV, straight for HAP. Everything else (H.264, ProRes, HEVC …) through the app's
//!   [`NativeThumbnailSource`] (AVFoundation's image generator in Swift).
//! - **Which frame.** DXV / HAP: frames `0 ..< min(15, round(fps × 1 s), frames)`; native: times `i / 15` s for
//!   `i in 0..15`, stopping at the first failure. The first frame whose 32 × 32 reduction has a mean luminance above
//!   0.10 or a brightest channel above 0.30 wins; else the brightest (by mean) seen ([`frame_stats`]).
//! - **Bake.** A checkerboard of `max(8, long edge / 10)` px tiles, greys 0.85 / 0.65, anchored at the bottom-left
//!   corner (Core Graphics' origin), the frame drawn over it (source-over); the result is opaque.
//! - **Sizes.** Long edge at most 1280 ([`crate::thumbkey::downscaled_size`]) with a Lanczos-3 filter (Swift:
//!   Core Graphics' high-quality interpolation — same size, not the same bytes) before the disk and memory tiers;
//!   JPEG quality 0.85.

use crate::header::{extract_info, is_dxv_fourcc, is_hap_fourcc};
use crate::thumbkey::{THUMBNAIL_LONG_EDGE, checkerboard_tile, downscaled_size, thumbnail_key};
use glance_media::AlphaView;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, RwLock};

/// Frames tried by the walk (`ThumbnailExtractor.frameCap`).
pub const FRAME_CAP: usize = 15;
/// Seconds of clip the walk covers (`secondsCap`).
pub const SECONDS_CAP: f64 = 1.0;
/// `meanLuminanceFloor`.
pub const MEAN_LUMINANCE_FLOOR: f64 = 0.10;
/// `maxLuminanceFloor`.
pub const MAX_LUMINANCE_FLOOR: f64 = 0.30;
/// Decodes at once, app-wide (`ConcurrencyLimiter(maxSlots: 4)`).
pub const CONCURRENT_DECODES: usize = 4;
/// The native path's size cap (`targetSize`, 1920 × 1080).
pub const NATIVE_MAX_SIZE: (u32, u32) = (1920, 1080);
/// JPEG quality (`kCGImageDestinationLossyCompressionQuality: 0.85`).
pub const JPEG_QUALITY: u8 = 85;
/// The memory tier's hard byte ceiling (1 GiB) and entry cap.
pub const MEMORY_BYTE_BUDGET: usize = 1 << 30;
pub const MEMORY_COUNT_LIMIT: usize = 500;
/// The checkerboard greys as Core Graphics lays `CGColor(gray: 0.85)` / `0.65` (generic grey, gamma 2.2) into the
/// extractor's DeviceRGB context (measured through Swift MaCrate's own extractor: 224 / 181, not 0.85 × 255).
pub const CHECKER_LIGHT: u8 = 224;
pub const CHECKER_DARK: u8 = 181;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// An RGBA picture, 8 bits per channel, rows top to bottom, `width * 4` bytes per row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    fn valid(&self) -> bool {
        self.width > 0 && self.height > 0 && self.pixels.len() == self.width as usize * self.height as usize * 4
    }
}

/// How a frame's colour is to be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alpha {
    /// Colour already multiplied by alpha (DXV's default, Arena's).
    Premultiplied,
    /// Colour independent of alpha (HAP, DXV marked Straight, the native path).
    Straight,
}

impl From<AlphaView> for Alpha {
    fn from(v: AlphaView) -> Alpha {
        match v {
            AlphaView::Premultiplied => Alpha::Premultiplied,
            AlphaView::Straight => Alpha::Straight,
        }
    }
}

/// The app's frame grabber for codecs the platform decodes (Swift: `AVAssetImageGenerator` with
/// `appliesPreferredTrackTransform`, zero time tolerance, `maximumSize` 1920 × 1080, `copyCGImage(at:)`).
pub trait NativeThumbnailSource: Send + Sync {
    /// The frame shown at `time` seconds, no larger than `max_size` (aspect kept, track transform applied), as
    /// RGBA — straight unless [`NativeThumbnailSource::premultiplied`] says otherwise. An error past the end of the
    /// clip (the walk stops there) or when the file cannot be read.
    fn frame_at(&self, path: &Path, time: f64, max_size: (u32, u32)) -> Result<Rgba, String>;

    /// Whether [`NativeThumbnailSource::frame_at`]'s colour is premultiplied (the bytes Core Graphics drew).
    fn premultiplied(&self) -> bool {
        false
    }
}

/// The native path on macOS: DXVKit Plus's `FrameGrabber` (AVFoundation's image generator set up as Swift MaCrate
/// sets it up), each time asked as `CMTime(seconds:preferredTimescale: 600)`, the premultiplied bytes Core
/// Graphics drew. One grabber per thread, kept while the same clip is asked for.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, Default)]
pub struct AvFoundationThumbnails;

#[cfg(target_os = "macos")]
thread_local! {
    static GRABBER: std::cell::RefCell<Option<(PathBuf, glance_media::FrameGrabber)>> = const { std::cell::RefCell::new(None) };
}

#[cfg(target_os = "macos")]
impl NativeThumbnailSource for AvFoundationThumbnails {
    fn frame_at(&self, path: &Path, time: f64, max_size: (u32, u32)) -> Result<Rgba, String> {
        GRABBER.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.as_ref().is_none_or(|(p, _)| p != path) {
                let g = glance_media::FrameGrabber::open(path, f64::from(max_size.0), f64::from(max_size.1)).map_err(|e| e.to_string())?;
                *slot = Some((path.to_path_buf(), g));
            }
            let Some((_, grabber)) = slot.as_mut() else { return Err("no grabber".to_string()) };
            let img = grabber.grab_seconds(time, 600, glance_media::GrabAlpha::Premultiplied).map_err(|e| e.to_string())?;
            Ok(Rgba { width: img.width, height: img.height, pixels: img.rgba })
        })
    }

    fn premultiplied(&self) -> bool {
        true
    }
}

/// Why no thumbnail was made (`ThumbnailExtractor.ThumbnailError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThumbnailError {
    Unreadable(String),
    DecodeFailed,
    /// A native-codec clip with no [`NativeThumbnailSource`] set.
    Unsupported,
}

impl std::fmt::Display for ThumbnailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThumbnailError::Unreadable(s) => write!(f, "unreadable: {s}"),
            ThumbnailError::DecodeFailed => f.write_str("decode failed"),
            ThumbnailError::Unsupported => f.write_str("no native thumbnail source"),
        }
    }
}

impl std::error::Error for ThumbnailError {}

// MARK: - Frame statistics, bake, downscale

/// The source pixel Core Graphics' default-quality draw samples for output pixel `i` (of 32) along an axis of
/// `dim` pixels, in memory order — measured through Swift MaCrate's own `imageStats` context on noise pictures
/// (tools/swift-library-oracle, `reduce`): the pixel just before the output pixel's centre `(i + ½) × dim / 32`
/// (`⌈c⌉ − 1`), except that the last output pixel takes the last source pixel when `Float(32 / dim) × dim` falls
/// short of 32 (1080, 2160, 700 … but not 1920, 720, 3840).
fn cg_sample_index(i: usize, dim: usize) -> usize {
    let c = (i as f64 + 0.5) * dim as f64 / 32.0;
    let mut k = (c.ceil() as usize).saturating_sub(1);
    if i == 31 && f64::from(32.0f32 / dim as f32) * (dim as f64) < 32.0 {
        k = dim - 1;
    }
    k.min(dim - 1)
}

/// `imageStats`: the frame drawn into a 32 × 32 premultiplied context — so a straight frame's colour is multiplied
/// by its alpha (rounded), and four sampled pixels in a row that are all transparent stay black — then the mean of R, G and B over all pixels
/// (`Σ / (1024 × 3 × 255)`, Swift's exact arithmetic) and the largest channel (`/ 255`). Pictures at least 192 px
/// on both sides (every real clip) are point-sampled as Core Graphics does (`cg_sample_index`); smaller ones,
/// which Core Graphics interpolates, are area-averaged here (close, not exact).
pub fn frame_stats(img: &Rgba, alpha: Alpha) -> (f64, f64) {
    if !img.valid() {
        return (1.0, 1.0);
    }
    const N: usize = 32;
    let (w, h) = (img.width as usize, img.height as usize);
    let premul = |p: &[u8]| -> [u8; 3] {
        let a = u32::from(p[3]);
        match alpha {
            Alpha::Premultiplied => [p[0], p[1], p[2]],
            Alpha::Straight => [0, 1, 2].map(|k| ((u32::from(p[k]) * a + 127) / 255) as u8),
        }
    };
    let mut sum = 0u64;
    let mut max = 0u8;
    if w >= 6 * N && h >= 6 * N {
        for j in 0..N {
            let y = cg_sample_index(j, h);
            let row: Vec<&[u8]> = (0..N).map(|i| (y * w + cg_sample_index(i, w)) * 4).map(|o| &img.pixels[o..o + 4]).collect();
            for (i, p) in row.iter().enumerate() {
                // Core Graphics leaves a four-pixel group whose alphas are all 0 untouched (black here).
                if p[3] == 0 && row[i / 4 * 4..i / 4 * 4 + 4].iter().all(|q| q[3] == 0) {
                    continue;
                }
                for v in premul(p) {
                    sum += u64::from(v);
                    max = max.max(v);
                }
            }
        }
    } else {
        // Area average with fractional coverage.
        let spans = |src: usize| -> Vec<Vec<(usize, f64)>> {
            (0..N)
                .map(|o| {
                    let a = o as f64 * src as f64 / N as f64;
                    let b = (o + 1) as f64 * src as f64 / N as f64;
                    let mut v = Vec::new();
                    let mut i = a.floor() as usize;
                    while (i as f64) < b && i < src {
                        let lo = a.max(i as f64);
                        let hi = b.min((i + 1) as f64);
                        if hi > lo {
                            v.push((i, (hi - lo) / (b - a)));
                        }
                        i += 1;
                    }
                    v
                })
                .collect()
        };
        let (cols, rows) = (spans(w), spans(h));
        for row in &rows {
            for col in &cols {
                let mut acc = [0.0f64; 3];
                for &(y, wy) in row {
                    for &(x, wx) in col {
                        let o = (y * w + x) * 4;
                        let c = premul(&img.pixels[o..o + 4]);
                        for k in 0..3 {
                            acc[k] += f64::from(c[k]) * wx * wy;
                        }
                    }
                }
                for v in acc {
                    let b = v.round().clamp(0.0, 255.0) as u8;
                    sum += u64::from(b);
                    max = max.max(b);
                }
            }
        }
    }
    let mean = sum as f64 / ((N * N * 3) as f64 * 255.0);
    (mean, f64::from(max) / 255.0)
}

/// `isContentful`.
pub fn is_contentful(stats: (f64, f64)) -> bool {
    stats.0 > MEAN_LUMINANCE_FLOOR || stats.1 > MAX_LUMINANCE_FLOOR
}

/// `compositeOntoCheckerboard`: the frame over a checkerboard of [`checkerboard_tile`] px tiles, light at the
/// bottom-left tile; opaque RGBA out. Core Graphics' 8-bit source-over, as measured on Swift MaCrate's output: a
/// straight frame is first premultiplied (`round(c × a / 255)`); then `min(255, c + round(d × (255 − a) / 255))` —
/// except that Core Graphics works four pixels at a time (aligned to the row start) and leaves a group whose four
/// alphas are all 0 untouched, so a premultiplied colour under alpha 0 is added only beside a visible pixel
/// (`transparent_group`).
pub fn bake_checkerboard(img: &Rgba, alpha: Alpha) -> Rgba {
    let (w, h) = (img.width, img.height);
    let cell = checkerboard_tile(w, h).max(1);
    let mut out = vec![0u8; img.pixels.len()];
    let div = |x: u32| (x + 127) / 255;
    for y in 0..h {
        // Core Graphics fills from the bottom row up.
        let row = (h - 1 - y) / cell;
        for x in 0..w {
            let light = (row + x / cell).is_multiple_of(2);
            let d = u32::from(if light { CHECKER_LIGHT } else { CHECKER_DARK });
            let o = ((y * w + x) * 4) as usize;
            let a = u32::from(img.pixels[o + 3]);
            for k in 0..3 {
                let c = u32::from(img.pixels[o + k]);
                let s = match alpha {
                    Alpha::Premultiplied => c,
                    Alpha::Straight => div(c * a),
                };
                out[o + k] = if a == 0 && transparent_group(img, x, y) { d as u8 } else { (s + div(d * (255 - a))).min(255) as u8 };
            }
            out[o + 3] = 255;
        }
    }
    Rgba { width: w, height: h, pixels: out }
}

/// Whether the four-pixel group (aligned to the row start) holding `(x, y)` is fully transparent: Core Graphics
/// skips such a group when compositing.
fn transparent_group(img: &Rgba, x: u32, y: u32) -> bool {
    let x0 = x / 4 * 4;
    (x0..(x0 + 4).min(img.width)).all(|xx| img.pixels[((y * img.width + xx) * 4 + 3) as usize] == 0)
}

/// `ThumbnailDownscale.downscaleIfNeeded`: unchanged when the long edge is at most 1280, else resized to
/// [`downscaled_size`] with a Lanczos-3 filter.
pub fn downscale_if_needed(img: Rgba) -> Rgba {
    if img.width.max(img.height) <= THUMBNAIL_LONG_EDGE || !img.valid() {
        return img;
    }
    let (nw, nh) = downscaled_size(img.width, img.height);
    let (w, h) = (img.width, img.height);
    match image::RgbaImage::from_raw(w, h, img.pixels) {
        Some(buf) => {
            let small = image::imageops::resize(&buf, nw, nh, image::imageops::FilterType::Lanczos3);
            Rgba { width: nw, height: nh, pixels: small.into_raw() }
        }
        None => Rgba { width: 0, height: 0, pixels: Vec::new() },
    }
}

/// The JPEG bytes of an opaque picture (alpha dropped), quality 0.85.
pub fn encode_jpeg(img: &Rgba) -> Option<Vec<u8>> {
    if !img.valid() {
        return None;
    }
    let rgb: Vec<u8> = img.pixels.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode(&rgb, img.width, img.height, image::ExtendedColorType::Rgb8).ok()?;
    Some(out)
}

/// Decode a cached JPEG to RGBA.
pub fn decode_jpeg(bytes: &[u8]) -> Option<Rgba> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).ok()?.to_rgba8();
    Some(Rgba { width: img.width(), height: img.height(), pixels: img.into_raw() })
}

// MARK: - Extractor

/// A counting gate (`ConcurrencyLimiter`): at most `n` holders.
#[derive(Debug)]
struct Limiter {
    free: Mutex<usize>,
    cv: Condvar,
}

impl Limiter {
    fn new(n: usize) -> Limiter {
        Limiter { free: Mutex::new(n), cv: Condvar::new() }
    }
    fn acquire(&self) -> LimiterSlot<'_> {
        let mut f = lock(&self.free);
        while *f == 0 {
            f = self.cv.wait(f).unwrap_or_else(PoisonError::into_inner);
        }
        *f -= 1;
        LimiterSlot(self)
    }
}

struct LimiterSlot<'a>(&'a Limiter);

impl Drop for LimiterSlot<'_> {
    fn drop(&mut self) {
        *lock(&self.0.free) += 1;
        self.0.cv.notify_one();
    }
}

/// Which frame the walk picked, for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WalkResult {
    /// Frame index (DXV / HAP) or sample number `i` of `i / 15` s (native).
    pub index: usize,
    pub contentful: bool,
    pub mean: f64,
    pub max: f64,
}

/// `ThumbnailExtractor`: decodes, walks and bakes, at most [`CONCURRENT_DECODES`] at once.
pub struct ThumbnailExtractor {
    native: Option<Arc<dyn NativeThumbnailSource>>,
    limiter: Limiter,
}

impl std::fmt::Debug for ThumbnailExtractor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbnailExtractor").field("native", &self.native.is_some()).finish_non_exhaustive()
    }
}

impl ThumbnailExtractor {
    pub fn new(native: Option<Arc<dyn NativeThumbnailSource>>) -> ThumbnailExtractor {
        ThumbnailExtractor { native, limiter: Limiter::new(CONCURRENT_DECODES) }
    }

    /// `extract(url:alphaType:)`: the baked picture at the clip's own size (native: within 1920 × 1080).
    pub fn extract(&self, path: &Path, alpha_type: AlphaView) -> Result<Rgba, ThumbnailError> {
        self.extract_with_walk(path, alpha_type).map(|(img, _)| img)
    }

    /// [`ThumbnailExtractor::extract`], and which frame it took.
    pub fn extract_with_walk(&self, path: &Path, alpha_type: AlphaView) -> Result<(Rgba, WalkResult), ThumbnailError> {
        let _slot = self.limiter.acquire();
        let fourcc = glance_media::first_video_fourcc(path);
        let (frame, alpha, walk) = match fourcc {
            Some(cc) if is_dxv_fourcc(&cc) => {
                let a = Alpha::from(alpha_type);
                let (f, w) = walk_decoded(path, a)?;
                (f, a, w)
            }
            Some(cc) if is_hap_fourcc(&cc) => {
                let (mut f, w) = walk_decoded(path, Alpha::Straight)?;
                if &cc == b"HapA" {
                    hap_alpha_only_as_white(&mut f);
                }
                (f, Alpha::Straight, w)
            }
            _ => {
                let native = self.native.as_ref().ok_or(ThumbnailError::Unsupported)?;
                let a = if native.premultiplied() { Alpha::Premultiplied } else { Alpha::Straight };
                let (f, w) = walk_native(native.as_ref(), path, a)?;
                (f, a, w)
            }
        };
        Ok((bake_checkerboard(&frame, alpha), walk))
    }
}

/// HAP Alpha-only (`HapA`): GlanceCore's `decodeHapAToRGBA` makes it white with the stored value as alpha; DXVKit
/// Plus hands the value as grey.
fn hap_alpha_only_as_white(img: &mut Rgba) {
    for p in img.pixels.as_chunks_mut::<4>().0 {
        let v = p[0];
        *p = [255, 255, 255, v];
    }
}

/// The walk over decoded frames (DXV / HAP): `walkForwardDXV3` / `walkForwardHAP`. Frames that fail to decode are
/// skipped.
fn walk_decoded(path: &Path, alpha: Alpha) -> Result<(Rgba, WalkResult), ThumbnailError> {
    let clip = glance_media::route(path).map_err(|e| ThumbnailError::Unreadable(e.to_string()))?;
    let mut source = clip.open().map_err(|e| ThumbnailError::Unreadable(e.to_string()))?;
    let total = clip.info.frames as usize;
    if total == 0 {
        return Err(ThumbnailError::DecodeFailed);
    }
    let fps = extract_info(path).fps.filter(|f| *f > 0.0).unwrap_or(30.0);
    let one_second = ((fps * SECONDS_CAP).round() as usize).max(1);
    let cap = FRAME_CAP.min(one_second).min(total);
    let mut best: Option<(Rgba, WalkResult)> = None;
    for i in 0..cap {
        let Ok(decoded) = source.decode(i as u32) else { continue };
        let Ok(pixels) = source.cpu_rgba(&decoded) else { continue };
        let (width, height) = decoded.size();
        let img = Rgba { width, height, pixels };
        if !img.valid() {
            continue;
        }
        let stats = frame_stats(&img, alpha);
        let walk = WalkResult { index: i, contentful: is_contentful(stats), mean: stats.0, max: stats.1 };
        if walk.contentful {
            return Ok((img, walk));
        }
        if best.as_ref().is_none_or(|(_, b)| stats.0 > b.mean) {
            best = Some((img, walk));
        }
    }
    best.ok_or(ThumbnailError::DecodeFailed)
}

/// The native walk (`extractAVAsset`): times `i / 15 × 1 s`; the first failure ends it.
fn walk_native(native: &dyn NativeThumbnailSource, path: &Path, alpha: Alpha) -> Result<(Rgba, WalkResult), ThumbnailError> {
    let mut best: Option<(Rgba, WalkResult)> = None;
    let mut last_error = None;
    for i in 0..FRAME_CAP {
        let t = i as f64 / FRAME_CAP as f64 * SECONDS_CAP;
        match native.frame_at(path, t, NATIVE_MAX_SIZE) {
            Ok(img) if img.valid() => {
                let stats = frame_stats(&img, alpha);
                let walk = WalkResult { index: i, contentful: is_contentful(stats), mean: stats.0, max: stats.1 };
                if walk.contentful {
                    return Ok((img, walk));
                }
                if best.as_ref().is_none_or(|(_, b)| stats.0 > b.mean) {
                    best = Some((img, walk));
                }
            }
            Ok(_) => {
                last_error = Some("empty frame".to_string());
                break;
            }
            Err(e) => {
                last_error = Some(e);
                break;
            }
        }
    }
    match (best, last_error) {
        (Some(b), _) => Ok(b),
        (None, Some(e)) => Err(ThumbnailError::Unreadable(e)),
        (None, None) => Err(ThumbnailError::DecodeFailed),
    }
}

// MARK: - Disk cache

/// `ThumbnailCache`: `<dir>/<key>.jpg`.
#[derive(Clone, Debug)]
pub struct ThumbnailCache {
    dir: PathBuf,
}

/// The key of the file at `path` (the path string as given — see [`crate::thumbkey::thumbnail_key_for`] for which
/// form Swift used where): size and modification date read without following a symbolic link.
pub fn cache_key(path: &str) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    let size = i64::try_from(meta.len()).ok()?;
    Some(thumbnail_key(path, size, crate::enumerate::foundation_mtime(&meta)?))
}

impl ThumbnailCache {
    /// Creates the folder (`<caches>/<bundle id>/thumbs`).
    pub fn new(dir: &Path) -> ThumbnailCache {
        let _ = std::fs::create_dir_all(dir);
        ThumbnailCache { dir: dir.to_path_buf() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self, path: &str) -> Option<PathBuf> {
        cache_key(path).map(|k| self.dir.join(format!("{k}.jpg")))
    }

    /// `image(for:)`: the cached picture, if any.
    pub fn image(&self, path: &str) -> Option<Rgba> {
        decode_jpeg(&std::fs::read(self.file(path)?).ok()?)
    }

    /// `store(_:for:)`: downscale, encode, write (through a temporary file).
    pub fn store(&self, img: &Rgba, path: &str) {
        let Some(file) = self.file(path) else { return };
        let Some(jpeg) = encode_jpeg(&downscale_if_needed(img.clone())) else { return };
        let _ = crate::roots_store::write_atomically(&file, &jpeg);
    }

    /// `evict(for:)`.
    pub fn evict(&self, path: &str) {
        if let Some(f) = self.file(path) {
            let _ = std::fs::remove_file(f);
        }
    }

    /// `cachedJPEGData(for:)`: the JPEG bytes as cached (the Brief feature sends them), never extracting.
    pub fn cached_jpeg_data(&self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.file(path)?).ok()
    }
}

// MARK: - Memory cache

struct MemEntry {
    image: Arc<Rgba>,
    cost: usize,
    stamp: u64,
}

#[derive(Default)]
struct MemState {
    entries: HashMap<String, MemEntry>,
    order: BTreeMap<u64, String>,
    total: usize,
    next: u64,
}

/// `MemoryThumbnailCache`: an LRU keyed like the disk tier, holding downscaled pictures; evicts on insert until
/// both the byte budget and the count cap hold (a single entry larger than the budget is kept).
pub struct MemoryThumbnailCache {
    byte_budget: usize,
    count_limit: usize,
    state: Mutex<MemState>,
}

impl std::fmt::Debug for MemoryThumbnailCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = lock(&self.state);
        f.debug_struct("MemoryThumbnailCache").field("entries", &s.entries.len()).field("bytes", &s.total).finish()
    }
}

impl Default for MemoryThumbnailCache {
    fn default() -> Self {
        MemoryThumbnailCache::new(MEMORY_BYTE_BUDGET, MEMORY_COUNT_LIMIT)
    }
}

impl MemoryThumbnailCache {
    pub fn new(byte_budget: usize, count_limit: usize) -> MemoryThumbnailCache {
        MemoryThumbnailCache { byte_budget, count_limit, state: Mutex::new(MemState::default()) }
    }

    /// Entries and bytes held.
    pub fn usage(&self) -> (usize, usize) {
        let s = lock(&self.state);
        (s.entries.len(), s.total)
    }

    fn touch(s: &mut MemState, key: &str) {
        let stamp = s.next;
        s.next += 1;
        if let Some(e) = s.entries.get_mut(key) {
            s.order.remove(&e.stamp);
            e.stamp = stamp;
            s.order.insert(stamp, key.to_string());
        }
    }

    /// `image(for:)`: a hit becomes most recently used.
    pub fn get(&self, key: &str) -> Option<Arc<Rgba>> {
        let mut s = lock(&self.state);
        let img = s.entries.get(key)?.image.clone();
        Self::touch(&mut s, key);
        Some(img)
    }

    /// `store(_:for:)` of an already-downscaled picture; cost = `width * 4 * height`.
    pub fn insert(&self, key: &str, image: Arc<Rgba>) {
        let cost = (image.width as usize * 4 * image.height as usize).max(1);
        let mut s = lock(&self.state);
        if let Some(old) = s.entries.remove(key) {
            s.order.remove(&old.stamp);
            s.total -= old.cost;
        }
        let stamp = s.next;
        s.next += 1;
        s.entries.insert(key.to_string(), MemEntry { image, cost, stamp });
        s.order.insert(stamp, key.to_string());
        s.total += cost;
        while (s.total > self.byte_budget || s.entries.len() > self.count_limit) && s.entries.len() > 1 {
            let Some((_, victim)) = s.order.pop_first() else { break };
            if let Some(e) = s.entries.remove(&victim) {
                s.total -= e.cost;
            }
        }
    }

    /// `remove(for:)`.
    pub fn remove(&self, key: &str) {
        let mut s = lock(&self.state);
        if let Some(e) = s.entries.remove(key) {
            s.order.remove(&e.stamp);
            s.total -= e.cost;
        }
    }
}

// MARK: - Service

type AlphaProvider = Arc<dyn Fn(&str) -> Option<AlphaView> + Send + Sync>;

#[derive(Default)]
struct Inflight {
    result: Mutex<Option<Option<Arc<Rgba>>>>,
    done: Condvar,
}

impl Inflight {
    fn wait(&self) -> Option<Arc<Rgba>> {
        let mut r = lock(&self.result);
        loop {
            if let Some(v) = r.as_ref() {
                return v.clone();
            }
            r = self.done.wait(r).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// `ThumbnailService`: memory → disk → one shared extraction per path in flight → store. Blocking; call it off
/// the UI thread (four extractions run at once; more callers wait).
pub struct ThumbnailService {
    cache: ThumbnailCache,
    memory: MemoryThumbnailCache,
    extractor: ThumbnailExtractor,
    inflight: Mutex<HashMap<String, Arc<Inflight>>>,
    alpha: RwLock<Option<AlphaProvider>>,
}

impl std::fmt::Debug for ThumbnailService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbnailService").field("cache", &self.cache).field("memory", &self.memory).finish_non_exhaustive()
    }
}

impl ThumbnailService {
    /// A service over the cache folder `dir` (created), with the app's native frame grabber.
    pub fn new(dir: &Path, native: Option<Arc<dyn NativeThumbnailSource>>) -> ThumbnailService {
        ThumbnailService {
            cache: ThumbnailCache::new(dir),
            memory: MemoryThumbnailCache::default(),
            extractor: ThumbnailExtractor::new(native),
            inflight: Mutex::new(HashMap::new()),
            alpha: RwLock::new(None),
        }
    }

    /// `setAlphaTypeProvider`: the clip's stored Alpha Type by path (MaCrate: `MetadataStore::alpha_type_for_path`);
    /// `None` = the default, Premultiplied.
    pub fn set_alpha_type_provider(&self, provider: Option<AlphaProvider>) {
        *self.alpha.write().unwrap_or_else(PoisonError::into_inner) = provider;
    }

    pub fn cache(&self) -> &ThumbnailCache {
        &self.cache
    }

    pub fn memory(&self) -> &MemoryThumbnailCache {
        &self.memory
    }

    /// `thumbnail(for:)`: the poster of the clip at `path` (the path string the key is made from), downscaled;
    /// `None` when it cannot be made (the cell shows its placeholder; nothing is remembered about the failure).
    pub fn thumbnail(&self, path: &str) -> Option<Arc<Rgba>> {
        let key = cache_key(path);
        if let Some(k) = &key
            && let Some(hit) = self.memory.get(k)
        {
            return Some(hit);
        }
        if let Some(disk) = self.cache.image(path) {
            let img = Arc::new(downscale_if_needed(disk));
            if let Some(k) = &key {
                self.memory.insert(k, img.clone());
            }
            return Some(img);
        }
        let (job, owner) = {
            let mut map = lock(&self.inflight);
            match map.get(path) {
                Some(j) => (j.clone(), false),
                None => {
                    let j = Arc::new(Inflight::default());
                    map.insert(path.to_string(), j.clone());
                    (j, true)
                }
            }
        };
        if !owner {
            return job.wait();
        }
        let provider = self.alpha.read().unwrap_or_else(PoisonError::into_inner).clone();
        let alpha = provider.and_then(|p| p(path)).unwrap_or_default();
        let result = match self.extractor.extract(Path::new(path), alpha) {
            Ok(img) => {
                self.cache.store(&img, path);
                let small = Arc::new(downscale_if_needed(img));
                if let Some(k) = cache_key(path) {
                    self.memory.insert(&k, small.clone());
                }
                Some(small)
            }
            Err(_) => None,
        };
        *lock(&job.result) = Some(result.clone());
        job.done.notify_all();
        lock(&self.inflight).remove(path);
        result
    }

    /// `regenerate(_:)`: the clip's Alpha Type changed — wait out an extraction in flight, drop both tiers, extract
    /// again.
    pub fn regenerate(&self, path: &str) -> Option<Arc<Rgba>> {
        let existing = lock(&self.inflight).get(path).cloned();
        if let Some(j) = existing {
            let _ = j.wait();
        }
        if let Some(k) = cache_key(path) {
            self.memory.remove(&k);
        }
        self.cache.evict(path);
        self.thumbnail(path)
    }
}

// MARK: - Prefetch

/// `ThumbnailPrefetcher`: warm the cache for a whole list, four at a time, in list order; a new list cancels the
/// previous one; paused while the preview plays (the list is kept and re-issued on resume).
pub struct ThumbnailPrefetcher {
    service: Arc<ThumbnailService>,
    generation: Arc<AtomicU64>,
    pending: Mutex<Vec<String>>,
    paused: AtomicBool,
}

impl std::fmt::Debug for ThumbnailPrefetcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbnailPrefetcher").field("paused", &self.paused.load(Ordering::SeqCst)).finish_non_exhaustive()
    }
}

impl ThumbnailPrefetcher {
    pub fn new(service: Arc<ThumbnailService>) -> ThumbnailPrefetcher {
        ThumbnailPrefetcher { service, generation: Arc::new(AtomicU64::new(0)), pending: Mutex::new(Vec::new()), paused: AtomicBool::new(false) }
    }

    /// `prefetch(_:)`: remember the list; unless paused, cancel the running prefetch and start this one on four
    /// threads (detached; they stop at the next clip once cancelled).
    pub fn prefetch(&self, paths: Vec<String>) {
        *lock(&self.pending) = paths.clone();
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if self.paused.load(Ordering::SeqCst) {
            return;
        }
        let list = Arc::new(paths);
        let next = Arc::new(Mutex::new(0usize));
        for _ in 0..4 {
            let (svc, gen_now, list, next) = (self.service.clone(), self.generation.clone(), list.clone(), next.clone());
            std::thread::spawn(move || {
                loop {
                    if gen_now.load(Ordering::SeqCst) != generation {
                        break;
                    }
                    let i = {
                        let mut n = lock(&next);
                        let i = *n;
                        *n += 1;
                        i
                    };
                    let Some(p) = list.get(i) else { break };
                    let _ = svc.thumbnail(p);
                }
            });
        }
    }

    /// `cancel()`.
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// `pause()`: cancel and refuse new work until [`ThumbnailPrefetcher::resume`].
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        self.cancel();
    }

    /// `resume()`: re-issue the most recent list.
    pub fn resume(&self) {
        if !self.paused.swap(false, Ordering::SeqCst) {
            return;
        }
        let pending = lock(&self.pending).clone();
        if !pending.is_empty() {
            self.prefetch(pending);
        }
    }
}

// MARK: - The one-time cache rebuild

/// The completion marker in Application Support (`ThumbnailMigration.markerName`).
pub const MIGRATION_MARKER: &str = ".thumb-migration-v0.15.0.done";
/// The legacy `UserDefaults` flag (`ThumbnailMigration.doneKey`), honoured once and then written forward.
pub const MIGRATION_LEGACY_KEY: &str = "Crate.thumb.v0.15.0.migration.done";

/// `isMigrationDone()`: the marker file exists; or the legacy flag is set, and then the marker is written now.
pub fn is_migration_done(app_support: &Path, legacy_flag: bool) -> bool {
    if app_support.join(MIGRATION_MARKER).exists() {
        return true;
    }
    if legacy_flag {
        write_migration_marker(app_support);
        return true;
    }
    false
}

/// `writeMarker()`: an empty file.
pub fn write_migration_marker(app_support: &Path) {
    let _ = std::fs::create_dir_all(app_support);
    let _ = std::fs::write(app_support.join(MIGRATION_MARKER), b"");
}

/// What a migration run did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// Already done (marker or legacy flag): the cache is kept.
    AlreadyDone,
    /// The library has clips but none is reachable (drives offline): nothing deleted, not marked done.
    SkippedOffline,
    /// The cache folder is not `…/Library/Caches/…/thumbs`: not deleted, not marked done.
    UnexpectedCacheDir,
    /// Deleted and rebuilt; this many clips processed (failures count); marker written.
    Finished(usize),
}

/// `ThumbnailMigration.performMigration` for the paths of the library's non-orphan rows: unless done, delete the
/// cache folder (guarded) and re-thumbnail every clip four at a time, `progress(completed, total)` every 16 clips
/// and at the end; then write the marker (the app also sets its legacy flag, as Swift does).
pub fn run_migration(
    service: &ThumbnailService,
    paths: &[String],
    app_support: &Path,
    legacy_flag: bool,
    mut progress: impl FnMut(usize, usize),
) -> MigrationOutcome {
    if is_migration_done(app_support, legacy_flag) {
        return MigrationOutcome::AlreadyDone;
    }
    if !paths.is_empty() && !paths.iter().any(|p| Path::new(p).exists()) {
        return MigrationOutcome::SkippedOffline;
    }
    let dir = service.cache().dir().to_path_buf();
    let d = dir.to_string_lossy();
    if !(d.contains("/Library/Caches/") && d.trim_end_matches('/').ends_with("/thumbs")) {
        return MigrationOutcome::UnexpectedCacheDir;
    }
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let total = paths.len();
    progress(0, total);
    let next = Mutex::new(0usize);
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let mut completed = 0;
    std::thread::scope(|s| {
        for _ in 0..4 {
            let tx = tx.clone();
            let next = &next;
            s.spawn(move || {
                loop {
                    let i = {
                        let mut n = lock(next);
                        let i = *n;
                        *n += 1;
                        i
                    };
                    let Some(p) = paths.get(i) else { break };
                    let _ = service.thumbnail(p);
                    if tx.send(()).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        for () in rx {
            completed += 1;
            if completed % 16 == 0 {
                progress(completed, total);
            }
        }
    });
    progress(completed, total);
    write_migration_marker(app_support);
    MigrationOutcome::Finished(completed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, p: [u8; 4]) -> Rgba {
        Rgba { width: w, height: h, pixels: p.repeat((w * h) as usize) }
    }

    #[test]
    fn stats_of_flat_frames() {
        assert_eq!(frame_stats(&solid(64, 36, [0, 0, 0, 255]), Alpha::Premultiplied), (0.0, 0.0));
        let (m, x) = frame_stats(&solid(64, 36, [255, 255, 255, 255]), Alpha::Straight);
        assert_eq!((m, x), (1.0, 1.0));
        // Straight colour under alpha 0 counts as black; premultiplied colour is taken as it is.
        assert_eq!(frame_stats(&solid(10, 10, [255, 255, 255, 0]), Alpha::Straight), (0.0, 0.0));
        assert_eq!(frame_stats(&solid(10, 10, [51, 51, 51, 51]), Alpha::Premultiplied).1, 0.2);
        assert!(!is_contentful((0.10, 0.30)));
        assert!(is_contentful((0.1001, 0.0)) && is_contentful((0.0, 0.31)));
    }

    #[test]
    fn a_small_bright_logo_is_contentful() {
        let mut img = solid(320, 180, [0, 0, 0, 255]);
        for y in 80..100 {
            for x in 150..170 {
                let o = ((y * 320 + x) * 4) as usize;
                img.pixels[o..o + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
        let s = frame_stats(&img, Alpha::Premultiplied);
        assert!(s.0 < MEAN_LUMINANCE_FLOOR && s.1 > MAX_LUMINANCE_FLOOR, "{s:?}");
    }

    #[test]
    fn checkerboard_is_anchored_bottom_left() {
        let img = solid(100, 20, [0, 0, 0, 0]);
        let out = bake_checkerboard(&img, Alpha::Premultiplied);
        let at = |x: u32, y: u32| out.pixels[((y * 100 + x) * 4) as usize];
        // Tile 10 px; the bottom-left tile (x 0..10, y 10..20 from the top) is light.
        assert_eq!(at(0, 19), CHECKER_LIGHT);
        assert_eq!(at(0, 9), CHECKER_DARK);
        assert_eq!(at(10, 19), CHECKER_DARK);
        assert!(out.pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        // Opaque frames cover it.
        let red = bake_checkerboard(&solid(4, 4, [200, 0, 0, 255]), Alpha::Straight);
        assert!(red.pixels.as_chunks::<4>().0.iter().all(|p| *p == [200, 0, 0, 255]));
    }

    #[test]
    fn downscale_and_jpeg_round_trip() {
        let big = solid(1920, 1080, [10, 200, 30, 255]);
        let small = downscale_if_needed(big);
        assert_eq!((small.width, small.height), (1280, 720));
        let jpeg = encode_jpeg(&small).expect("jpeg");
        let back = decode_jpeg(&jpeg).expect("decode");
        assert_eq!((back.width, back.height), (1280, 720));
        let p = &back.pixels[0..4];
        assert!(p[0].abs_diff(10) <= 3 && p[1].abs_diff(200) <= 3 && p[2].abs_diff(30) <= 3 && p[3] == 255, "{p:?}");
        let tiny = solid(16, 16, [255, 0, 0, 255]);
        assert_eq!(downscale_if_needed(tiny.clone()), tiny);
    }

    #[test]
    fn memory_lru_evicts_oldest_and_keeps_one_oversize() {
        let m = MemoryThumbnailCache::new(1000, 3);
        let img = |w| Arc::new(solid(w, 1, [0; 4]));
        m.insert("a", img(50)); // 200 bytes
        m.insert("b", img(50));
        m.insert("c", img(50));
        assert!(m.get("a").is_some()); // a is now most recent
        m.insert("d", img(50)); // count 4 > 3: evict b (oldest)
        assert!(m.get("b").is_none() && m.get("a").is_some() && m.get("c").is_some() && m.get("d").is_some());
        m.insert("e", img(200)); // 800 bytes: 1400 > 1000 → the two oldest go, d and e stay (1000)
        assert_eq!(m.usage(), (2, 1000));
        assert!(m.get("d").is_some() && m.get("e").is_some());
        m.insert("huge", img(1000)); // 4000 > budget, kept alone
        assert_eq!(m.usage(), (1, 4000));
        m.remove("huge");
        assert_eq!(m.usage(), (0, 0));
    }

    #[test]
    fn migration_marker_rules() {
        let d = std::env::temp_dir().join(format!("glance-thumbmig-{}", uuid::Uuid::new_v4()));
        assert!(!is_migration_done(&d, false));
        assert!(is_migration_done(&d, true), "the legacy flag is honoured");
        assert!(d.join(MIGRATION_MARKER).exists(), "and written forward");
        assert!(is_migration_done(&d, false));
        std::fs::remove_dir_all(&d).ok();
    }
}
