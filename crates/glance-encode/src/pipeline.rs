//! The encode of one job (Swift GlEnc 1.6.2 `CoreEncoder.makePipeline` + `EncodePipeline.run`): open the source,
//! check the crop and the size, build the sink, then frame by frame read → crop → resize → encode → write, with
//! the trim window, a progress report after every frame and a cancel check before every frame. A failed or
//! cancelled encode removes its partial output (`EncodeQueue.swift:1019-1032`, the CLI's `:180`).
//!
//! Frames in flight (v0.8.0): the source is read in file order under one lock; each worker decodes the frame it
//! took, crops, resizes and encodes it with its own encoder (DXV / HAP: every packet depends only on its own frame —
//! the DXV packer's tables are cleared per frame — so it is the same packet whichever worker makes it); the calling
//! thread writes the results strictly in source order, reports progress after each and checks for cancel before
//! each, and an error stops the job at its frame, as the one-frame-at-a-time loop of v0.7.2 and Swift did. A window
//! bounds the frames taken but not yet written.

use crate::EncodeError;
use crate::audio::{AudioPlan, Pcm};
use crate::codec::{DxvFormat, OutputCodec, OutputContainer, VideoSettings};
use crate::frame::{CropRect, Frame, FrameAlpha};
use crate::size::{AspectMode, OutputSize, ResizeQuality};
use crate::source::{AlphaType, SourceKind, SourceReader, open_reader, source_kind};
use dxvkit_plus_enc::dxv::{DxvEffort, DxvEncoder, DxvVariant};
use dxvkit_plus_enc::hap::{HapCodec, HapEncoder};
use dxvkit_plus_enc::mov::MovWriter;
use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, mpsc};

/// Everything one encode needs (Swift's `EncodeRequest` plus the audio plan the queue resolves).
#[derive(Clone, Debug, PartialEq)]
pub struct EncodeRequest {
    pub source: PathBuf,
    pub output: PathBuf,
    pub codec: OutputCodec,
    pub container: OutputContainer,
    pub video: VideoSettings,
    pub output_size: OutputSize,
    pub resize_quality: ResizeQuality,
    pub aspect: AspectMode,
    pub crop: Option<CropRect>,
    /// Source frames `[start, end)`; `None` = the whole clip.
    pub frames: Option<Range<u64>>,
    /// The `udta/©swr` string of DXV / HAP outputs (`GlEnc <version>`; the CLI's `glenc-cli 1.0.0`).
    pub writer_version: String,
    /// HAP chunk count 1…64 (ignored by other codecs).
    pub hap_chunks: u32,
    /// What a DXV source's colour is (ignored by other sources).
    pub alpha_type: AlphaType,
    pub audio: AudioPlan,
    /// HAP's refined BC4 endpoint search (Swift's session-only `BC4Config.useRefinement`, off by default).
    pub refined_bc4: bool,
}

/// What a finished encode reports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EncodeOutcome {
    /// Frames written.
    pub frames: u64,
    /// The audio was not written, and why (the row shows "Done · no audio").
    pub audio_warning: Option<String>,
}

/// The tag frames of a native source carry for this output (`CoreEncoder.makeDXVEncoder`, `:116-124`, `:182`):
/// `.last` (straight) for outputs that keep alpha, else `.noneSkipLast` (opaque).
pub fn source_alpha_tag(codec: OutputCodec) -> FrameAlpha {
    if codec.has_alpha() { FrameAlpha::Straight } else { FrameAlpha::Opaque }
}

/// What a worker made of one frame, for the writer.
pub(crate) enum Made {
    /// A DXV / HAP packet.
    Packet(Vec<u8>),
    /// A frame in BGRA for the platform's writer.
    Picture(Frame),
}

/// One worker's encoder (every worker has its own).
pub(crate) trait Maker: Send {
    fn make(&mut self, frame: Frame) -> Result<Made, EncodeError>;
}

/// Where frames go: the workers' encoders, and the writer that takes their results in source order.
pub(crate) trait Sink {
    /// An encoder for one more worker.
    fn maker(&self) -> Result<Box<dyn Maker>, EncodeError>;
    fn write(&mut self, made: Made) -> Result<(), EncodeError>;
    fn finish(self: Box<Self>) -> Result<Option<String>, EncodeError>;
}

enum Packets {
    Dxv(Box<DxvEncoder>),
    Hap(Box<HapEncoder>),
}

/// DXV / HAP: our encoders, our QuickTime writer, the PCM attached at the end (`DXVEncoderSink`).
struct PacketSink {
    format: DxvFormat,
    size: (u32, u32),
    hap_chunks: u32,
    refined_bc4: bool,
    /// Each encoder's threads for one frame.
    threads: usize,
    writer: MovWriter,
    audio: Option<Pcm>,
}

struct PacketMaker {
    format: DxvFormat,
    encoder: Packets,
}

impl Maker for PacketMaker {
    fn make(&mut self, frame: Frame) -> Result<Made, EncodeError> {
        let packet = match &mut self.encoder {
            Packets::Dxv(e) => {
                // `DXVKitFrameEncoder.fillRGBA`: opaque variants take RGB as read with alpha 255; alpha variants the
                // colour through the frame's alpha mode. In place: the frame is this worker's own.
                let (opaque, alpha) = (!self.format.has_alpha(), frame.alpha);
                let mut rgba = frame.rgba;
                for px in rgba.as_chunks_mut::<4>().0 {
                    *px = if opaque { [px[0], px[1], px[2], 255] } else { dxvkit_plus_enc::alpha::normalize(alpha, *px) };
                }
                e.encode(&rgba)?
            }
            Packets::Hap(e) => e.encode(&frame.rgba, frame.alpha)?,
        };
        Ok(Made::Packet(packet))
    }
}

impl Sink for PacketSink {
    fn maker(&self) -> Result<Box<dyn Maker>, EncodeError> {
        let (w, h) = self.size;
        let encoder = if let Some(v) = dxv_variant(self.format) {
            let mut e = DxvEncoder::new(v, w, h, DxvEffort::Best)?;
            e.set_threads(self.threads);
            Packets::Dxv(Box::new(e))
        } else if let Some(c) = hap_codec(self.format) {
            let mut e = HapEncoder::new(c, self.hap_chunks, w, h)?;
            e.set_refined_bc4(self.refined_bc4);
            e.set_threads(self.threads);
            Packets::Hap(Box::new(e))
        } else {
            return Err(EncodeError::Invalid(format!("{:?} is not a DXV / HAP format", self.format)));
        };
        Ok(Box::new(PacketMaker { format: self.format, encoder }))
    }
    fn write(&mut self, made: Made) -> Result<(), EncodeError> {
        let Made::Packet(p) = made else { return Err(EncodeError::Write("a picture for the packet writer".into())) };
        self.writer.append(&p)?;
        Ok(())
    }
    fn finish(self: Box<Self>) -> Result<Option<String>, EncodeError> {
        let mut w = self.writer;
        if let Some(a) = self.audio {
            w.attach_audio(a.rate, a.channels, a.data);
        }
        w.finish()?;
        Ok(None)
    }
}

fn dxv_variant(f: DxvFormat) -> Option<DxvVariant> {
    match f {
        DxvFormat::Dxt1 => Some(DxvVariant::Dxt1),
        DxvFormat::Dxt5 => Some(DxvVariant::Dxt5),
        DxvFormat::Ycg6 => Some(DxvVariant::Ycg6),
        DxvFormat::Yg10 => Some(DxvVariant::Yg10),
        _ => None,
    }
}

fn hap_codec(f: DxvFormat) -> Option<HapCodec> {
    match f {
        DxvFormat::Hap1 => Some(HapCodec::Hap1),
        DxvFormat::Hap5 => Some(HapCodec::Hap5),
        DxvFormat::HapY => Some(HapCodec::HapY),
        DxvFormat::HapA => Some(HapCodec::HapA),
        DxvFormat::HapM => Some(HapCodec::HapM),
        _ => None,
    }
}

fn make_sink(req: &EncodeRequest, (w, h): (u32, u32), fps: f64, audio: Option<Pcm>, bgra: bool, threads: usize) -> Result<Box<dyn Sink>, EncodeError> {
    match req.codec {
        OutputCodec::Dxv(format) => {
            if dxv_variant(format).is_none() && hap_codec(format).is_none() {
                return Err(EncodeError::Invalid(format!("{format:?} is not a DXV / HAP format")));
            }
            let writer = MovWriter::create(&req.output, format.stream_fourcc(), w, h, fps, &req.writer_version)?;
            Ok(Box::new(PacketSink { format, size: (w, h), hap_chunks: req.hap_chunks, refined_bc4: req.refined_bc4, threads, writer, audio }))
        }
        codec => crate::platform::av_sink(req, codec, w, h, audio, bgra),
    }
}

/// Workers and each encoder's threads for one frame, on `cores` cores for `frames` frames (0: not known) (v0.8.0):
/// a worker per core, each encoding on its own thread; a job of fewer frames than cores gets one worker per frame
/// and the spare cores inside each frame. The platform's writer encodes on its own: its workers only decode, crop,
/// resize and swap.
fn plan(cores: usize, frames: u64, platform_writer: bool) -> (usize, usize) {
    let cores = cores.max(1);
    let workers = if platform_writer { cores.min(PLATFORM_WORKERS) } else { cores };
    let frames = if frames == 0 { usize::MAX } else { usize::try_from(frames).unwrap_or(usize::MAX) };
    let workers = workers.min(frames).max(1);
    (workers, (cores / workers).max(1))
}

/// Workers in front of the platform's writer (it encodes each frame itself, in order).
const PLATFORM_WORKERS: usize = 4;

/// Swift's guards on the crop and the size (`EncodePipeline.swift:243-287`), with its error words.
fn check_geometry(req: &EncodeRequest, src: (u32, u32)) -> Result<(u32, u32), EncodeError> {
    let align = req.codec.dimension_alignment();
    if let Some(r) = req.crop {
        if !r.width.is_multiple_of(align) || !r.height.is_multiple_of(align) || r.width == 0 || r.height == 0 {
            return Err(EncodeError::Invalid(format!(
                "EncodePipeline: cropRect (x={}, y={}, {}×{}) must have integer-valued 4-pixel-aligned coords (L3); got non-aligned",
                r.x, r.y, r.width, r.height
            )));
        }
        let inside = u64::from(r.x) + u64::from(r.width) <= u64::from(src.0) && u64::from(r.y) + u64::from(r.height) <= u64::from(src.1);
        if !inside {
            return Err(EncodeError::Invalid(format!(
                "EncodePipeline: cropRect ({}.0, {}.0, {}.0, {}.0) extends outside source dims {}×{}",
                r.x, r.y, r.width, r.height, src.0, src.1
            )));
        }
    }
    let effective = req.crop.map_or(src, |r| (r.width, r.height));
    let out = req.output_size.resolved(effective);
    if req.output_size != OutputSize::Original && (!out.0.is_multiple_of(align) || !out.1.is_multiple_of(align)) {
        return Err(EncodeError::Invalid(format!(
            "EncodePipeline: non-.original output dimensions ({}×{}) must be 4-pixel-multiples (L3); got non-aligned",
            out.0, out.1
        )));
    }
    Ok(out)
}

/// Encode one job. `progress` gets 0…1 after every written frame and 1.0 at the end; `cancel` is checked before every
/// frame (the frame in flight is finished, as Swift's `Task.checkCancellation()` placement does). On any error the
/// partial output is removed.
pub fn encode(req: &EncodeRequest, progress: &dyn Fn(f64), cancel: &AtomicBool) -> Result<EncodeOutcome, EncodeError> {
    let r = run(req, progress, cancel);
    if r.is_err() {
        let _ = std::fs::remove_file(&req.output);
    }
    r
}

fn run(req: &EncodeRequest, progress: &dyn Fn(f64), cancel: &AtomicBool) -> Result<EncodeOutcome, EncodeError> {
    // A native source's frame that reaches the platform's writer unchanged stays in the decoder's BGRA.
    let platform_writer = !matches!(req.codec, OutputCodec::Dxv(_));
    let native = !matches!(source_kind(&req.source), Some((_, SourceKind::Dxv | SourceKind::Hap)));
    let bgra = platform_writer && native && req.crop.is_none() && req.output_size == OutputSize::Original;
    let reader = open_reader(&req.source, source_alpha_tag(req.codec), req.alpha_type, bgra)?;
    let info = reader.info();
    let (ow, oh) = check_geometry(req, (info.width, info.height))?;
    let total = info.total_frames;
    let (lo, hi) = match &req.frames {
        Some(r) => (r.start, if total > 0 { total.min(r.end) } else { r.end }),
        None => (0, total),
    };
    let (audio, warning) = crate::audio::prepare(&req.source, req.audio, req.container);
    // Trimmed jobs: the audio of the kept frames only (the "fix" of 2026-11-14), on the source's frame grid.
    let audio = match (audio, &req.frames) {
        (Some(a), Some(_)) if info.fps > 0.0 => Some(a.slice_seconds(lo as f64 / info.fps, hi as f64 / info.fps)).filter(|p| !p.data.is_empty()),
        (a, _) => a,
    };
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let (workers, threads) = plan(cores, hi.saturating_sub(lo), platform_writer);
    let mut sink = make_sink(req, (ow, oh), info.fps, audio, bgra, threads)?;
    let written = in_flight(req, reader, &mut *sink, (ow, oh), (lo, hi), workers, progress, cancel)?;
    let sink_warning = sink.finish()?;
    progress(1.0);
    Ok(EncodeOutcome { frames: written, audio_warning: warning.or(sink_warning) })
}

/// The reader's side of the workers: the next source frame to take, and whether taking has stopped.
struct Shared {
    reader: Box<dyn SourceReader>,
    next: u64,
    stopped: bool,
}

/// Frames in flight: `workers` workers, the calling thread writing in source order. Returns the frames written.
#[allow(clippy::too_many_arguments)]
fn in_flight(
    req: &EncodeRequest,
    reader: Box<dyn SourceReader>,
    sink: &mut dyn Sink,
    (ow, oh): (u32, u32),
    (lo, hi): (u64, u64),
    workers: usize,
    progress: &dyn Fn(f64),
    cancel: &AtomicBool,
) -> Result<u64, EncodeError> {
    let makers = (0..workers).map(|_| sink.maker()).collect::<Result<Vec<_>, _>>()?;
    // Frames taken but not written stay below `window` (at 4K: about 33 MB each in RGBA).
    let window = (2 * workers) as u64;
    let shared = Mutex::new(Shared { reader, next: 0, stopped: false });
    let room = Condvar::new();
    // The next source index to write.
    let writing = AtomicU64::new(lo);
    let stop = |shared: &Mutex<Shared>| {
        if let Ok(mut g) = shared.lock() {
            g.stopped = true;
        }
    };
    let (tx, rx) = mpsc::channel::<(u64, Result<Made, EncodeError>)>();
    let denom = hi.saturating_sub(lo).max(1) as f64;
    std::thread::scope(|s| {
        for mut maker in makers {
            let (tx, shared, room, writing) = (tx.clone(), &shared, &room, &writing);
            s.spawn(move || {
                loop {
                    // Take the next frame in file order (the read under the lock), within the window.
                    let (idx, item) = {
                        let Ok(mut g) = shared.lock() else { return };
                        while !g.stopped && !cancel.load(Ordering::Relaxed) && g.next >= writing.load(Ordering::Acquire).saturating_add(window) {
                            let Ok(next) = room.wait(g) else { return };
                            g = next;
                        }
                        if g.stopped || cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        if hi > 0 && g.next >= hi {
                            g.stopped = true;
                            room.notify_all();
                            return;
                        }
                        let idx = g.next;
                        g.next = idx.saturating_add(1);
                        match g.reader.next_pending() {
                            Ok(Some(p)) => (idx, p),
                            Ok(None) => {
                                g.stopped = true;
                                room.notify_all();
                                return;
                            }
                            Err(e) => {
                                g.stopped = true;
                                room.notify_all();
                                drop(g);
                                let _ = tx.send((idx, Err(e.into())));
                                return;
                            }
                        }
                    };
                    // Before the trim window: read, never decoded.
                    if idx < lo {
                        continue;
                    }
                    let made = item.frame().map_err(EncodeError::from).and_then(|frame| {
                        let cropped = match req.crop {
                            Some(r) => frame.crop(r)?,
                            None => frame,
                        };
                        let out = match req.output_size {
                            OutputSize::Original => cropped,
                            _ => cropped.resize(ow, oh, req.resize_quality, req.aspect)?,
                        };
                        maker.make(out)
                    });
                    let failed = made.is_err();
                    if failed {
                        stop(shared);
                        room.notify_all();
                    }
                    if tx.send((idx, made)).is_err() || failed {
                        return;
                    }
                }
            });
        }
        drop(tx);
        // The writer: source order, progress after each frame, cancel before each.
        let mut pending: BTreeMap<u64, Result<Made, EncodeError>> = BTreeMap::new();
        let mut next = lo;
        let result = loop {
            if cancel.load(Ordering::Relaxed) {
                break Err(EncodeError::Cancelled);
            }
            if let Some(r) = pending.remove(&next) {
                if let Err(e) = r.and_then(|made| sink.write(made)) {
                    break Err(e);
                }
                next = next.saturating_add(1);
                writing.store(next, Ordering::Release);
                room.notify_all();
                progress((next.saturating_sub(lo) as f64 / denom).min(1.0));
                continue;
            }
            match rx.recv() {
                Ok((idx, r)) => {
                    pending.insert(idx, r);
                }
                // Every worker has ended: cancelled, or every frame they made is written.
                Err(_) if cancel.load(Ordering::Relaxed) => break Err(EncodeError::Cancelled),
                Err(_) if pending.is_empty() => break Ok(next.saturating_sub(lo)),
                Err(_) => break Err(EncodeError::Write(format!("frame {next} was never made"))),
            }
        };
        stop(&shared);
        room.notify_all();
        result
    })
}

/// The output path a request writes (for callers that clean up themselves).
pub fn output_path(req: &EncodeRequest) -> &Path {
    &req.output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Pts;
    use crate::source::{Pending, SourceError, SourceInfo};
    use std::sync::Arc;
    use std::time::Duration;

    /// `n` frames of 4×4, frame `i`'s bytes all `i`; its decode sleeps a scrambled time so workers finish out of
    /// order; frame `fail_at` fails to decode, and the read of frame `read_fail_at` fails.
    struct Fake {
        n: u64,
        next: u64,
        fail_at: Option<u64>,
        read_fail_at: Option<u64>,
        taken: Arc<AtomicU64>,
    }

    impl SourceReader for Fake {
        fn info(&self) -> SourceInfo {
            SourceInfo {
                kind: crate::source::SourceKind::Dxv,
                fourcc: *b"DXD3",
                width: 4,
                height: 4,
                fps: 30.0,
                total_frames: self.n,
                unpremultiplies_on_read: false,
            }
        }
        fn next_pending(&mut self) -> Result<Option<Pending>, SourceError> {
            let i = self.next;
            if i >= self.n {
                return Ok(None);
            }
            self.next += 1;
            self.taken.fetch_max(i + 1, Ordering::SeqCst);
            if Some(i) == self.read_fail_at {
                return Err(SourceError::DxvDecodeFailed { index: i, why: "read".into() });
            }
            let fail = Some(i) == self.fail_at;
            Ok(Some(Pending::Decode(Box::new(move || {
                std::thread::sleep(Duration::from_micros((i * 7919 % 13) * 300));
                if fail {
                    return Err(SourceError::DxvDecodeFailed { index: i, why: "decode".into() });
                }
                Ok(Frame {
                    width: 4,
                    height: 4,
                    rgba: vec![i as u8; 64],
                    alpha: FrameAlpha::Straight,
                    pts: Pts { value: i as i64, timescale: 30 },
                    attachments: None,
                })
            }))))
        }
    }

    /// Records what is written; the frame's first byte is its index.
    struct Record(Arc<Mutex<Vec<u8>>>);
    struct Pass;
    impl Maker for Pass {
        fn make(&mut self, frame: Frame) -> Result<Made, EncodeError> {
            Ok(Made::Packet(frame.rgba))
        }
    }
    impl Sink for Record {
        fn maker(&self) -> Result<Box<dyn Maker>, EncodeError> {
            Ok(Box::new(Pass))
        }
        fn write(&mut self, made: Made) -> Result<(), EncodeError> {
            let Made::Packet(p) = made else { return Err(EncodeError::Write("picture".into())) };
            self.0.lock().map_err(|_| EncodeError::Write("lock".into()))?.push(p[0]);
            Ok(())
        }
        fn finish(self: Box<Self>) -> Result<Option<String>, EncodeError> {
            Ok(None)
        }
    }

    fn request() -> EncodeRequest {
        EncodeRequest {
            source: PathBuf::new(),
            output: PathBuf::new(),
            codec: OutputCodec::Dxv(DxvFormat::Dxt1),
            container: OutputContainer::Mov,
            video: VideoSettings::default(),
            output_size: OutputSize::Original,
            resize_quality: ResizeQuality::Auto,
            aspect: AspectMode::Letterbox,
            crop: None,
            frames: None,
            writer_version: String::new(),
            hap_chunks: 1,
            alpha_type: AlphaType::Premultiplied,
            audio: crate::audio::AudioPlan::default(),
            refined_bc4: false,
        }
    }

    struct Run {
        result: Result<u64, EncodeError>,
        written: Vec<u8>,
        progress: Vec<f64>,
        taken: u64,
    }

    fn go(n: u64, lo_hi: (u64, u64), workers: usize, fail_at: Option<u64>, read_fail_at: Option<u64>, cancel_after: Option<usize>) -> Run {
        let taken = Arc::new(AtomicU64::new(0));
        let reader = Box::new(Fake { n, next: 0, fail_at, read_fail_at, taken: taken.clone() });
        let out = Arc::new(Mutex::new(Vec::new()));
        let mut sink = Record(out.clone());
        let cancel = AtomicBool::new(false);
        let seen = Mutex::new(Vec::new());
        let progress = |p: f64| {
            if let Ok(mut s) = seen.lock() {
                s.push(p);
                if cancel_after == Some(s.len()) {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
        };
        let result = in_flight(&request(), reader, &mut sink, (4, 4), lo_hi, workers, &progress, &cancel);
        let written = out.lock().map(|v| v.clone()).unwrap_or_default();
        Run { result, written, progress: seen.into_inner().unwrap_or_default(), taken: taken.load(Ordering::SeqCst) }
    }

    #[test]
    fn frames_are_written_in_source_order_on_any_worker_count() {
        for workers in [1, 2, 5, 18] {
            let r = go(40, (0, 40), workers, None, None, None);
            assert_eq!(r.result.ok(), Some(40), "{workers}");
            assert_eq!(r.written, (0..40).collect::<Vec<u8>>(), "{workers}");
            assert_eq!(r.progress.len(), 40);
            assert!(r.progress.windows(2).all(|w| w[0] < w[1]) && r.progress.last() == Some(&1.0), "{workers}: {:?}", r.progress);
        }
    }

    #[test]
    fn a_trim_window_writes_its_frames_only() {
        let r = go(40, (5, 12), 6, None, None, None);
        assert_eq!(r.result.ok(), Some(7));
        assert_eq!(r.written, (5..12).collect::<Vec<u8>>());
        // Nothing is read past the window's end.
        assert!(r.taken <= 12, "{}", r.taken);
    }

    #[test]
    fn an_error_stops_the_job_at_its_frame() {
        for workers in [1, 4, 18] {
            // A decode failure at frame 9: frames 0..9 written, then that frame's error, as one frame at a time did.
            let r = go(40, (0, 40), workers, Some(9), None, None);
            assert!(matches!(&r.result, Err(EncodeError::Source(SourceError::DxvDecodeFailed { index: 9, .. }))), "{workers}: {:?}", r.result.as_ref().err());
            assert_eq!(r.written, (0..9).collect::<Vec<u8>>(), "{workers}");
            // A read failure at frame 13.
            let r = go(40, (0, 40), workers, None, Some(13), None);
            assert!(matches!(&r.result, Err(EncodeError::Source(SourceError::DxvDecodeFailed { index: 13, .. }))), "{workers}");
            assert_eq!(r.written, (0..13).collect::<Vec<u8>>(), "{workers}");
        }
    }

    #[test]
    fn a_cancel_stops_before_the_next_frame_and_the_window_bounds_the_reads() {
        for workers in [1, 4, 18] {
            let r = go(400, (0, 400), workers, None, None, Some(3));
            assert!(matches!(r.result, Err(EncodeError::Cancelled)), "{workers}");
            assert_eq!(r.written, vec![0, 1, 2], "{workers}");
            // Frames taken but not written never exceed the window (2 per worker).
            assert!(r.taken <= 3 + 2 * workers as u64, "{workers}: {}", r.taken);
        }
    }

    #[test]
    fn workers_and_threads_by_job() {
        assert_eq!(plan(18, 1000, false), (18, 1));
        assert_eq!(plan(18, 3, false), (3, 6));
        assert_eq!(plan(18, 0, false), (18, 1));
        assert_eq!(plan(18, 1000, true), (PLATFORM_WORKERS, 4));
        assert_eq!(plan(1, 1000, false), (1, 1));
    }
}
