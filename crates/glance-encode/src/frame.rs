//! One frame between the reader and the encoder: tightly packed RGBA8 at its own size, its alpha tag and its
//! presentation time; the crop, the Nearest scale and the letterbox canvas (Swift GlEnc 1.6.2 `FrameCropper.swift`,
//! `FrameResizer.swift`). Bilinear and Lanczos are the platform's scaler ([`crate::platform`]).

use crate::EncodeError;
use crate::size::{AspectMode, ResizeQuality, letterbox_rect};
pub use dxvkit_plus_enc::alpha::FrameAlpha;

/// A presentation time as a rational (`value / timescale` seconds), as CoreMedia carries it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pts {
    pub value: i64,
    pub timescale: i32,
}

impl Pts {
    /// Swift's `CMTime(seconds:preferredTimescale: 600)` for the DXV / HAP readers: `seconds × 600` truncated
    /// (CoreMedia, measured with Swift 6.4.2 on macOS 27: 219.99999999999997 → 219, 219.6 → 219, 2.5 → 2) — so frame 11
    /// of a 15360 / 512 clip lands on 219, not 220.
    pub fn from_seconds_600(seconds: f64) -> Pts {
        Pts { value: (seconds * 600.0).trunc() as i64, timescale: 600 }
    }
}

/// The platform decoder's buffer attachments (colour primaries, matrix, colour space…). Swift GlEnc hands the
/// decoder's own buffer to AVAssetWriter when it neither crops nor resizes, and those attachments give the file its
/// `colr` atom and steer the encoder's colour matrix; any buffer it makes itself (a crop, a resize, an
/// un-premultiplied copy, a DXV / HAP decode) has none. So a frame carries them only while it is the decoder's.
#[cfg(target_os = "macos")]
pub type Attachments = dxvkit_plus_apple::FrameAttachments;
/// No attachments on other platforms yet.
#[cfg(not(target_os = "macos"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachments;

/// The platform decoder's own buffer, handed to the platform's writer unchanged (v0.8.1): Swift GlEnc appends the
/// reader's buffer when it neither crops, resizes nor un-premultiplies.
#[cfg(target_os = "macos")]
pub type Through = dxvkit_plus_apple::DecodedBuffer;
/// No platform decoder on other platforms yet.
#[cfg(not(target_os = "macos"))]
#[derive(Clone, Debug)]
pub struct Through;

/// One frame (RGBA8, top row first, no row padding).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub alpha: FrameAlpha,
    pub pts: Pts,
    /// The decoder's attachments, while this is the decoder's own buffer.
    pub attachments: Option<Attachments>,
}

/// A crop rectangle in source pixels, top-left origin (Swift's `CGRect` with integer values).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Frame {
    /// The row-wise copy of the crop region (`FrameCropper.swift:117-121`). The rectangle is checked by the
    /// pipeline before the first frame.
    pub fn crop(&self, r: CropRect) -> Result<Frame, EncodeError> {
        let inside = r.x.checked_add(r.width).is_some_and(|e| e <= self.width) && r.y.checked_add(r.height).is_some_and(|e| e <= self.height);
        if !inside || r.width == 0 || r.height == 0 {
            return Err(EncodeError::Invalid(format!("crop {r:?} outside {}x{}", self.width, self.height)));
        }
        let (sw, w) = (self.width as usize * 4, r.width as usize * 4);
        let mut out = Vec::with_capacity(w * r.height as usize);
        for y in 0..r.height as usize {
            let at = (r.y as usize + y) * sw + r.x as usize * 4;
            out.extend_from_slice(self.rgba.get(at..at + w).ok_or_else(|| EncodeError::Invalid("crop row".into()))?);
        }
        Ok(Frame { width: r.width, height: r.height, rgba: out, alpha: self.alpha, pts: self.pts, attachments: None })
    }

    /// Swift's Nearest (`FrameResizer.swift:346-357`): `srcX = x * srcW / dstW`, `srcY = y * srcH / dstH`.
    pub fn nearest(&self, dw: u32, dh: u32) -> Frame {
        let (sw, sh) = (u64::from(self.width), u64::from(self.height));
        let mut out = vec![0u8; dw as usize * dh as usize * 4];
        for y in 0..u64::from(dh) {
            let sy = (y * sh / u64::from(dh)) as usize;
            for x in 0..u64::from(dw) {
                let sx = (x * sw / u64::from(dw)) as usize;
                let s = (sy * self.width as usize + sx) * 4;
                let d = (y as usize * dw as usize + x as usize) * 4;
                if let (Some(src), Some(dst)) = (self.rgba.get(s..s + 4), out.get_mut(d..d + 4)) {
                    dst.copy_from_slice(src);
                }
            }
        }
        Frame { width: dw, height: dh, rgba: out, alpha: self.alpha, pts: self.pts, attachments: None }
    }

    /// Swift's `FrameResizer.resize`: letterbox onto an opaque black canvas when the aspect differs, else a plain
    /// scale; no scale at all when the size is already the target (the frame passes through untouched).
    pub fn resize(&self, dw: u32, dh: u32, quality: ResizeQuality, aspect: AspectMode) -> Result<Frame, EncodeError> {
        if dw == 0 || dh == 0 {
            return Err(EncodeError::Invalid(format!("resize target {dw}x{dh}")));
        }
        if aspect == AspectMode::Letterbox {
            let r = letterbox_rect((self.width, self.height), (dw, dh));
            if !r.fills((dw, dh)) {
                let inner = self.plain_resize(r.width, r.height, quality)?;
                let mut canvas = Vec::with_capacity(dw as usize * dh as usize * 4);
                for _ in 0..dw as usize * dh as usize {
                    canvas.extend_from_slice(&[0, 0, 0, 255]);
                }
                let (row, cw) = (r.width as usize * 4, dw as usize * 4);
                for y in 0..r.height as usize {
                    let d = (r.inset_y as usize + y) * cw + r.inset_x as usize * 4;
                    if let (Some(dst), Some(src)) = (canvas.get_mut(d..d + row), inner.rgba.get(y * row..(y + 1) * row)) {
                        dst.copy_from_slice(src);
                    }
                }
                return Ok(Frame { width: dw, height: dh, rgba: canvas, alpha: self.alpha, pts: self.pts, attachments: None });
            }
        }
        self.plain_resize(dw, dh, quality)
    }

    fn plain_resize(&self, dw: u32, dh: u32, quality: ResizeQuality) -> Result<Frame, EncodeError> {
        if (dw, dh) == (self.width, self.height) {
            return Ok(self.clone());
        }
        match quality.resolved((self.width, self.height), (dw, dh)) {
            ResizeQuality::Nearest => Ok(self.nearest(dw, dh)),
            q => {
                let rgba = crate::platform::scale(&self.rgba, (self.width, self.height), (dw, dh), q == ResizeQuality::Lanczos)?;
                Ok(Frame { width: dw, height: dh, rgba, alpha: self.alpha, pts: self.pts, attachments: None })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32) -> Frame {
        let rgba = (0..w * h).flat_map(|i| [(i % 256) as u8, (i / 256 % 256) as u8, 7, 200]).collect();
        Frame { width: w, height: h, rgba, alpha: FrameAlpha::Straight, pts: Pts { value: 0, timescale: 600 }, attachments: None }
    }

    #[test]
    fn swift_cmtime_truncates() {
        assert_eq!(Pts::from_seconds_600(11.0 * (512.0 / 15360.0)).value, 219);
        assert_eq!(Pts::from_seconds_600(219.6 / 600.0).value, 219);
        assert_eq!(Pts::from_seconds_600(2.5 / 600.0).value, 2);
        assert_eq!(Pts::from_seconds_600(20.00000000000001 / 600.0).value, 20);
    }

    #[test]
    fn crop_copies_the_region() {
        let f = frame(5, 4);
        let c = f.crop(CropRect { x: 1, y: 2, width: 3, height: 2 }).unwrap();
        assert_eq!((c.width, c.height), (3, 2));
        assert_eq!(&c.rgba[0..4], &f.rgba[(2 * 5 + 1) * 4..(2 * 5 + 1) * 4 + 4]);
        assert!(f.crop(CropRect { x: 3, y: 0, width: 3, height: 1 }).is_err());
    }

    #[test]
    fn nearest_follows_swift_index_rule() {
        let f = frame(4, 1);
        let n = f.nearest(3, 1);
        // srcX = 0, 1, 2 for dstW 3 of srcW 4 (x * 4 / 3).
        assert_eq!(n.rgba[4], f.rgba[4]);
        assert_eq!(n.rgba[8], f.rgba[8]);
    }

    #[test]
    fn same_size_passes_through() {
        let f = frame(8, 6);
        assert_eq!(f.resize(8, 6, ResizeQuality::Lanczos, AspectMode::Letterbox).unwrap(), f);
    }

    #[test]
    fn letterbox_bars_are_opaque_black() {
        let f = frame(16, 4);
        let r = f.resize(8, 8, ResizeQuality::Nearest, AspectMode::Letterbox).unwrap();
        assert_eq!(&r.rgba[0..4], &[0, 0, 0, 255]);
        assert_eq!(r.rgba.len(), 8 * 8 * 4);
    }
}
