//! A clip's duration, frame rate and display size from its container header, as Swift MaCrate reads them for the
//! index (`DurationExtractor.extractInfo`, CrateKit/DurationExtractor.swift) — without decoding a frame and
//! without the system's media frameworks.
//!
//! Routing is by the first video track's sample-entry FourCC (`DXVDetector.compressorFourCC`, here
//! [`glance_media::first_video_fourcc`]):
//! - DXV (`DXV3` `DXD3` `DXD4` `DXDI`) — the numbers GlanceCore's `DXVDemuxer` (DXVKit 1.0) gives: the media
//!   duration (`mdhd`), frames ÷ that duration, the sample-entry size. Read from DXVKit Plus's demux
//!   ([`glance_media::Movie`]) of the first DXV track, whose first packet must state a variant.
//! - HAP (`Hap1` `Hap5` `HapY` `HapM` `HapA`) — the numbers GlanceCore's `HAPDemuxer` gives: the last frame's start
//!   plus its `stts` delta, frames ÷ that, the sample-entry size. Read from DXVKit Plus's demux of the first video
//!   track.
//! - Everything else — [`container_info`], a port of MaCrate's own `ContainerDurationReader` (`moov > mvhd`, the
//!   first `vide` track's `mdhd` / `stts` / `stsd` / `tkhd`). Matroska, AVI, WebM and MXF have no `moov`: nothing.
//!
//! No decoder is copied here: DXV and HAP tables come from DXVKit Plus (through glance-media); only MaCrate's
//! arithmetic over them is ported. What a failure leaves: [`HeaderInfo`] with `None`s; the index stores the
//! duration as NULL (retried on the next pass) and a missing rate or size as 0 ("tried, none") —
//! [`HeaderInfo::stored_fps`], [`HeaderInfo::stored_size`].

use glance_media::{Codec, Movie, VideoTrack};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// What the header gives (`ContainerHeaderInfo`). Width and height are both set or both `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HeaderInfo {
    /// Seconds.
    pub duration: Option<f64>,
    /// Mean frames per second of the video track.
    pub fps: Option<f64>,
    /// Display size: DXV / HAP the demuxed size (never padded to 16), other codecs the sample-entry size turned
    /// by a quarter-turn track matrix.
    pub size: Option<(u32, u32)>,
}

impl HeaderInfo {
    /// Nothing obtainable.
    pub const NONE: HeaderInfo = HeaderInfo { duration: None, fps: None, size: None };

    /// The `clips.frame_rate` value the indexer writes: the rate, or 0 for "tried, none".
    pub fn stored_fps(&self) -> f64 {
        self.fps.unwrap_or(0.0)
    }

    /// The `clips.video_width` / `video_height` values the indexer writes: the size, or 0 × 0 for "tried, none".
    pub fn stored_size(&self) -> (i64, i64) {
        self.size.map_or((0, 0), |(w, h)| (i64::from(w), i64::from(h)))
    }
}

/// `DurationExtractor.isDXVFourCC`.
pub fn is_dxv_fourcc(fourcc: &[u8; 4]) -> bool {
    matches!(fourcc, b"DXV3" | b"DXD3" | b"DXD4" | b"DXDI")
}

/// `ThumbnailExtractor.isHAPFourCC` (HAP R `Hap7` and HAP HDR `HapH` are not HAP to Swift MaCrate: they take the
/// container path).
pub fn is_hap_fourcc(fourcc: &[u8; 4]) -> bool {
    matches!(fourcc, b"Hap1" | b"Hap5" | b"HapY" | b"HapM" | b"HapA")
}

/// `DurationExtractor.extractInfo`: duration, rate and size from one header read, routed by codec.
pub fn extract_info(path: &Path) -> HeaderInfo {
    match glance_media::first_video_fourcc(path) {
        Some(cc) if is_dxv_fourcc(&cc) => dxv_info(path).unwrap_or(HeaderInfo::NONE),
        Some(cc) if is_hap_fourcc(&cc) => hap_info(path).unwrap_or(HeaderInfo::NONE),
        _ => container_info(path),
    }
}

fn positive(v: f64) -> Option<f64> {
    (v.is_finite() && v > 0.0).then_some(v)
}

fn first_video_track(movie: &Movie, codec: Option<Codec>) -> Option<&VideoTrack> {
    // DXVKit 1.0 (DXV) stops at the first DXV track and fails when it is damaged; HAPDemuxer takes the first
    // video track and fails when it is not usable. DXVKit Plus keeps parsing past a damaged track, so the
    // damaged ones are counted here.
    let parsed = movie.tracks.iter().filter(|t| codec.is_none_or(|c| t.codec == c)).map(|t| (t.index, Some(t)));
    let damaged = movie.damaged.iter().filter(|d| codec.is_none_or(|c| d.codec() == Some(c))).map(|d| (d.index, None));
    parsed.chain(damaged).min_by_key(|(i, _)| *i).and_then(|(_, t)| t)
}

/// GlanceCore's `DXVDemuxer.demux` → `DurationExtractor.extractDXV3`.
fn dxv_info(path: &Path) -> Option<HeaderInfo> {
    let movie = Movie::open_all(path).ok()?;
    let track = first_video_track(&movie, Some(Codec::Dxv))?;
    let file_size = std::fs::metadata(path).ok()?.len();
    // DXVKit 1.0 refuses a table whose samples run past the end of the file; DXVKit Plus keeps them.
    if track.samples.iter().any(|&(off, size)| off > file_size || u64::from(size) > file_size - off) {
        return None;
    }
    let mut packet = Vec::new();
    movie.read_sample(track, 0, &mut packet).ok()?;
    glance_media::parse_dxv_header(&packet).ok()?;
    let duration = track.duration as f64 / f64::from(track.timescale.max(1));
    if duration.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return None;
    }
    let fps = track.frame_count() as f64 / duration;
    Some(HeaderInfo { duration: Some(duration), fps: positive(fps), size: Some((track.width, track.height)) })
}

/// GlanceCore's `HAPDemuxer.demux` → `DurationExtractor.extractHAP`: the duration is the last frame's start
/// (the `stts` deltas of the frames before it, the table's last delta repeated where it runs short) plus that
/// frame's own delta from the table (0 when the table does not reach it), each over the media time scale.
fn hap_info(path: &Path) -> Option<HeaderInfo> {
    let movie = Movie::open_all(path).ok()?;
    let track = first_video_track(&movie, None)?;
    if !is_hap_fourcc(&track.fourcc) || track.timescale == 0 {
        return None;
    }
    let frames = track.samples.len() as u64;
    let last = frames.checked_sub(1)?;
    let pad = track.stts.iter().rev().find(|&&(c, _)| c > 0).map_or(1, |&(_, d)| d);
    let mut before_last = 0u64; // ticks of the frames before the last
    let mut covered = 0u64;
    let mut last_delta = None;
    for &(count, delta) in &track.stts {
        let (count, delta) = (u64::from(count), u64::from(delta));
        if last < covered + count {
            before_last = before_last.checked_add((last - covered).checked_mul(delta)?)?;
            last_delta = Some(delta);
            break;
        }
        before_last = before_last.checked_add(count.checked_mul(delta)?)?;
        covered += count;
    }
    if last_delta.is_none() {
        before_last = before_last.checked_add((last - covered).checked_mul(u64::from(pad))?)?;
    }
    let last_delta = last_delta.unwrap_or(0);
    let ts = f64::from(track.timescale);
    let duration = before_last as f64 / ts + last_delta as f64 / ts;
    if duration.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return None;
    }
    let fps = frames as f64 / duration;
    Some(HeaderInfo { duration: Some(duration), fps: positive(fps), size: Some((track.width, track.height)) })
}

// MARK: - ContainerDurationReader

/// MaCrate's `ContainerDurationReader.info`: the movie duration (`mvhd`), and the first `vide` track's mean frame
/// rate (`sample count × mdhd time scale ÷ Σ stts count × delta`) and display size (`stsd` entry width / height at
/// 32 / 34, swapped when the `tkhd` matrix turns a quarter). The top level is scanned to the end, so a `moov`
/// after the media data is found; `size == 1` (64-bit size) and `size == 0` (to the end) boxes are read. Every
/// read is bounds-checked; anything malformed is `None` for its part.
pub fn container_info(path: &Path) -> HeaderInfo {
    let Ok(file) = File::open(path) else { return HeaderInfo::NONE };
    // `FileManager.attributesOfItem`: the path's own size (a symbolic link's, not its target's).
    let Some(size) = std::fs::symlink_metadata(path).ok().and_then(|m| i64::try_from(m.len()).ok()).filter(|&s| s > 0) else {
        return HeaderInfo::NONE;
    };
    let mut r = Reader { file };
    let Some(moov) = r.find_box(b"moov", 0, size) else { return HeaderInfo::NONE };
    let duration = r.find_box(b"mvhd", moov.start, moov.end).and_then(|b| r.mvhd(b));
    let (fps, size) = r.video_track(moov);
    HeaderInfo { duration, fps, size }
}

#[derive(Clone, Copy)]
struct Box {
    start: i64,
    end: i64,
}

struct Reader {
    file: File,
}

fn be32(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4).map_or(0, |s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn be64(b: &[u8], at: usize) -> u64 {
    let mut v = 0u64;
    for i in 0..8 {
        v = (v << 8) | u64::from(b.get(at + i).copied().unwrap_or(0));
    }
    v
}

impl Reader {
    /// Exactly `count` bytes at `offset`, or `None` (a short read included).
    fn read(&mut self, offset: i64, count: i64) -> Option<Vec<u8>> {
        if offset < 0 || count <= 0 {
            return None;
        }
        self.file.seek(SeekFrom::Start(u64::try_from(offset).ok()?)).ok()?;
        let mut buf = vec![0u8; usize::try_from(count).ok()?];
        self.file.read_exact(&mut buf).ok()?;
        Some(buf)
    }

    /// The first sibling box of `kind` in `[start, end)`; `None` on any malformed or short header before it.
    fn find_box(&mut self, kind: &[u8; 4], start: i64, end: i64) -> Option<Box> {
        let mut offset = start;
        while offset.checked_add(8)? <= end {
            let header = self.read(offset, 8)?;
            let mut size = i64::from(be32(&header, 0));
            let mut header_size = 8;
            if size == 1 {
                let large = be64(&self.read(offset + 8, 8)?, 0);
                size = i64::try_from(large).ok()?;
                header_size = 16;
            } else if size == 0 {
                size = end - offset;
            }
            // Swift traps on `offset + size` overflowing; here that is a malformed box.
            let box_end = offset.checked_add(size)?;
            if size < header_size || box_end > end {
                return None;
            }
            if header.get(4..8) == Some(kind.as_slice()) {
                return Some(Box { start: offset + header_size, end: box_end });
            }
            offset = box_end;
        }
        None
    }

    fn mvhd(&mut self, b: Box) -> Option<f64> {
        if b.start + 1 > b.end {
            return None;
        }
        let version = *self.read(b.start, 1)?.first()?;
        let (timescale, duration) = match version {
            0 => {
                if b.start + 20 > b.end {
                    return None;
                }
                (be32(&self.read(b.start + 12, 4)?, 0), u64::from(be32(&self.read(b.start + 16, 4)?, 0)))
            }
            1 => {
                if b.start + 32 > b.end {
                    return None;
                }
                (be32(&self.read(b.start + 20, 4)?, 0), be64(&self.read(b.start + 24, 8)?, 0))
            }
            _ => return None,
        };
        if timescale == 0 || duration == 0 {
            return None;
        }
        Some(duration as f64 / f64::from(timescale))
    }

    /// The first `vide` track's rate and size. A malformed first video track gives nothing rather than falling
    /// through to a second one.
    fn video_track(&mut self, moov: Box) -> (Option<f64>, Option<(u32, u32)>) {
        let mut start = moov.start;
        while let Some(trak) = self.find_box(b"trak", start, moov.end) {
            start = trak.end;
            let Some(mdia) = self.find_box(b"mdia", trak.start, trak.end) else { continue };
            let Some(hdlr) = self.find_box(b"hdlr", mdia.start, mdia.end) else { continue };
            if hdlr.start + 12 > hdlr.end || self.read(hdlr.start + 8, 4).as_deref() != Some(b"vide".as_slice()) {
                continue;
            }
            let Some(minf) = self.find_box(b"minf", mdia.start, mdia.end) else { return (None, None) };
            let Some(stbl) = self.find_box(b"stbl", minf.start, minf.end) else { return (None, None) };
            let mut fps = None;
            if let Some(mdhd) = self.find_box(b"mdhd", mdia.start, mdia.end)
                && let Some(timescale) = self.mdhd_timescale(mdhd)
                && let Some(stts) = self.find_box(b"stts", stbl.start, stbl.end)
            {
                fps = self.stts_mean_rate(stts, timescale);
            }
            let tkhd = self.find_box(b"tkhd", trak.start, trak.end);
            let size = self.find_box(b"stsd", stbl.start, stbl.end).and_then(|stsd| self.display_size(stsd, tkhd));
            return (fps, size);
        }
        (None, None)
    }

    fn display_size(&mut self, stsd: Box, tkhd: Option<Box>) -> Option<(u32, u32)> {
        if stsd.start + 8 + 36 > stsd.end {
            return None;
        }
        let wh = self.read(stsd.start + 8 + 32, 4)?;
        let w = u32::from(u16::from_be_bytes([*wh.first()?, *wh.get(1)?]));
        let h = u32::from(u16::from_be_bytes([*wh.get(2)?, *wh.get(3)?]));
        if w == 0 || h == 0 {
            return None;
        }
        let turned = tkhd.is_some_and(|t| self.quarter_turn(t));
        Some(if turned { (h, w) } else { (w, h) })
    }

    /// The `tkhd` matrix turns by 90 or 270 degrees: `a` and `d` are 0, `b` and `c` are not.
    fn quarter_turn(&mut self, tkhd: Box) -> bool {
        if tkhd.start + 1 > tkhd.end {
            return false;
        }
        let at = match self.read(tkhd.start, 1).and_then(|v| v.first().copied()) {
            Some(0) => 40,
            Some(1) => 52,
            _ => return false,
        };
        if tkhd.start + at + 36 > tkhd.end {
            return false;
        }
        let Some(m) = self.read(tkhd.start + at, 36) else { return false };
        let (a, b, c, d) = (be32(&m, 0), be32(&m, 4), be32(&m, 12), be32(&m, 16));
        a == 0 && d == 0 && b != 0 && c != 0
    }

    fn mdhd_timescale(&mut self, mdhd: Box) -> Option<u32> {
        if mdhd.start + 1 > mdhd.end {
            return None;
        }
        let at = match *self.read(mdhd.start, 1)?.first()? {
            0 => 12,
            1 => 20,
            _ => return None,
        };
        if mdhd.start + at + 4 > mdhd.end {
            return None;
        }
        let ts = be32(&self.read(mdhd.start + at, 4)?, 0);
        (ts > 0).then_some(ts)
    }

    fn stts_mean_rate(&mut self, stts: Box, timescale: u32) -> Option<f64> {
        if stts.start + 8 > stts.end {
            return None;
        }
        let entries = i64::from(be32(&self.read(stts.start + 4, 4)?, 0));
        if entries <= 0 || stts.start + 8 + entries * 8 > stts.end {
            return None;
        }
        let table = self.read(stts.start + 8, entries * 8)?;
        let (mut samples, mut ticks) = (0u64, 0u64);
        for e in table.as_chunks::<8>().0 {
            let count = u64::from(be32(e, 0));
            let delta = u64::from(be32(e, 4));
            samples = samples.checked_add(count)?;
            ticks = ticks.checked_add(count * delta)?;
        }
        if samples == 0 || ticks == 0 {
            return None;
        }
        positive(samples as f64 * f64::from(timescale) / ticks as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourcc_sets() {
        for cc in [b"DXV3", b"DXD3", b"DXD4", b"DXDI"] {
            assert!(is_dxv_fourcc(cc));
        }
        for cc in [b"Hap1", b"Hap5", b"HapY", b"HapM", b"HapA"] {
            assert!(is_hap_fourcc(cc));
        }
        assert!(!is_hap_fourcc(b"Hap7"));
        assert!(!is_hap_fourcc(b"HapH"));
        assert!(!is_dxv_fourcc(b"avc1"));
    }

    #[test]
    fn stored_sentinels() {
        assert_eq!(HeaderInfo::NONE.stored_fps(), 0.0);
        assert_eq!(HeaderInfo::NONE.stored_size(), (0, 0));
        let info = HeaderInfo { duration: Some(2.0), fps: Some(25.0), size: Some((1920, 1080)) };
        assert_eq!(info.stored_size(), (1920, 1080));
    }
}
