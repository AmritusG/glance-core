//! The encode of one job (Swift GlEnc 1.6.2 `CoreEncoder.makePipeline` + `EncodePipeline.run`): open the source,
//! check the crop and the size, build the sink, then frame by frame read → crop → resize → encode → write, with
//! the trim window, a progress report after every frame and a cancel check before every frame. A failed or
//! cancelled encode removes its partial output (`EncodeQueue.swift:1019-1032`, the CLI's `:180`).

use crate::EncodeError;
use crate::audio::{AudioPlan, Pcm};
use crate::codec::{DxvFormat, OutputCodec, OutputContainer, VideoSettings};
use crate::frame::{CropRect, Frame, FrameAlpha};
use crate::size::{AspectMode, OutputSize, ResizeQuality};
use crate::source::{AlphaType, SourceReader, open_source};
use dxvkit_plus_enc::dxv::{DxvEffort, DxvEncoder, DxvVariant};
use dxvkit_plus_enc::hap::{HapCodec, HapEncoder};
use dxvkit_plus_enc::mov::MovWriter;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

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

/// Where frames go.
pub(crate) trait Sink {
    fn consume(&mut self, frame: &Frame) -> Result<(), EncodeError>;
    fn finish(self: Box<Self>) -> Result<Option<String>, EncodeError>;
}

enum Packets {
    Dxv(Box<DxvEncoder>),
    Hap(Box<HapEncoder>),
}

/// DXV / HAP: our encoder, our QuickTime writer, the PCM attached at the end (`DXVEncoderSink`).
struct PacketSink {
    format: DxvFormat,
    encoder: Packets,
    writer: MovWriter,
    audio: Option<Pcm>,
    rgba: Vec<u8>,
}

impl Sink for PacketSink {
    fn consume(&mut self, frame: &Frame) -> Result<(), EncodeError> {
        let packet = match &mut self.encoder {
            Packets::Dxv(e) => {
                // `DXVKitFrameEncoder.fillRGBA`: opaque variants take RGB as read with alpha 255; alpha variants the
                // colour through the frame's alpha mode.
                self.rgba.clear();
                let opaque = !self.format.has_alpha();
                self.rgba.extend(
                    frame
                        .rgba
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .flat_map(|&px| if opaque { [px[0], px[1], px[2], 255] } else { dxvkit_plus_enc::alpha::normalize(frame.alpha, px) }),
                );
                e.encode(&self.rgba)?
            }
            Packets::Hap(e) => e.encode(&frame.rgba, frame.alpha)?,
        };
        self.writer.append(&packet)?;
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

fn make_sink(req: &EncodeRequest, w: u32, h: u32, fps: f64, audio: Option<Pcm>) -> Result<Box<dyn Sink>, EncodeError> {
    match req.codec {
        OutputCodec::Dxv(format) => {
            let encoder = if let Some(v) = dxv_variant(format) {
                Packets::Dxv(Box::new(DxvEncoder::new(v, w, h, DxvEffort::Best)?))
            } else if let Some(c) = hap_codec(format) {
                {
                    let mut e = HapEncoder::new(c, req.hap_chunks, w, h)?;
                    e.set_refined_bc4(req.refined_bc4);
                    Packets::Hap(Box::new(e))
                }
            } else {
                return Err(EncodeError::Invalid(format!("{format:?} is not a DXV / HAP format")));
            };
            let writer = MovWriter::create(&req.output, format.stream_fourcc(), w, h, fps, &req.writer_version)?;
            Ok(Box::new(PacketSink { format, encoder, writer, audio, rgba: Vec::new() }))
        }
        codec => crate::platform::av_sink(req, codec, w, h, audio),
    }
}

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
    let mut reader: Box<dyn SourceReader> = open_source(&req.source, source_alpha_tag(req.codec), req.alpha_type)?;
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
    let mut sink = make_sink(req, ow, oh, info.fps, audio)?;
    let denom = hi.saturating_sub(lo).max(1) as f64;
    let (mut src_idx, mut written) = (0u64, 0u64);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(EncodeError::Cancelled);
        }
        if hi > 0 && src_idx >= hi {
            break;
        }
        let Some(frame) = reader.next_frame()? else { break };
        if src_idx < lo {
            src_idx = src_idx.saturating_add(1);
            continue;
        }
        let cropped = match req.crop {
            Some(r) => frame.crop(r)?,
            None => frame,
        };
        let out = match req.output_size {
            OutputSize::Original => cropped,
            _ => cropped.resize(ow, oh, req.resize_quality, req.aspect)?,
        };
        sink.consume(&out)?;
        src_idx = src_idx.saturating_add(1);
        written = written.saturating_add(1);
        progress((written as f64 / denom).min(1.0));
    }
    let sink_warning = sink.finish()?;
    progress(1.0);
    Ok(EncodeOutcome { frames: written, audio_warning: warning.or(sink_warning) })
}

/// The output path a request writes (for callers that clean up themselves).
pub fn output_path(req: &EncodeRequest) -> &Path {
    &req.output
}
