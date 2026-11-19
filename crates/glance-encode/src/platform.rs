//! The platform's half of an encode: decoding the codecs the OS owns, the OS's writer for ProRes / H.264 / HEVC /
//! Motion JPEG, the OS's audio decode and resampling, and its scaler. On macOS this is DXVKit Plus's Apple island,
//! doing exactly what Swift GlEnc 1.6.2 does through AVFoundation and Accelerate. Elsewhere it says so (the Windows
//! backend arrives with the Windows milestone).

use crate::EncodeError;
use crate::audio::Pcm;
use crate::codec::OutputCodec;
use crate::frame::FrameAlpha;
use crate::pipeline::EncodeRequest;
use crate::source::{SourceError, SourceReader};
use std::path::Path;

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
    use crate::codec::{H264Profile, MJPEG_QUALITY, OutputContainer, ProResVariant, RateControl};
    use crate::frame::{Frame, Pts};
    use crate::pipeline::{Made, Maker};
    use crate::source::{Pending, SourceInfo, SourceKind, validate_geometry};
    use dxvkit_plus_apple as apple;

    fn source_err(e: apple::SourceReaderError) -> SourceError {
        match e {
            apple::SourceReaderError::NoVideoTrack => SourceError::NoVideoTrack,
            apple::SourceReaderError::AvAssetReaderStartFailed(s) => SourceError::ReaderStartFailed(s),
            apple::SourceReaderError::AvAssetReaderFailed(s) => SourceError::ReaderFailed(s),
            apple::SourceReaderError::SourceDimensionsInvalid { width, height } => SourceError::DimensionsInvalid { width, height },
            apple::SourceReaderError::SourceDimensionsTooLarge { width, height } => SourceError::DimensionsTooLarge { width, height },
            apple::SourceReaderError::SourceFrameTooLarge { width, height } => SourceError::FrameTooLarge { width, height },
            apple::SourceReaderError::SourceDurationInvalid(d) => SourceError::DurationInvalid(d),
            other => SourceError::ReaderStartFailed(other.to_string()),
        }
    }

    /// Swift's `AVAssetReaderSourceReader`: 32BGRA frames (swapped to RGBA unless `keep_bgra`), the rate by
    /// Glance's FrameTiming rule (DXVKit Plus's `FrameTiming`, equal to Swift's), frames counted as
    /// `round(duration × fps)` — for a fragmented file, the samples its fragments hold (v0.8.0).
    pub struct NativeSource {
        info: SourceInfo,
        reader: apple::BgraReader,
        alpha: FrameAlpha,
        keep_bgra: bool,
    }

    impl SourceReader for NativeSource {
        fn info(&self) -> SourceInfo {
            self.info
        }
        /// The decoder's copy is read in order; the swap and un-premultiply are the worker's.
        fn next_pending(&mut self) -> Result<Option<Pending>, SourceError> {
            let Some(f) = self.reader.next_frame().map_err(source_err)? else { return Ok(None) };
            let (alpha, swap, unpremultiply) = (self.alpha, !self.keep_bgra, self.info.unpremultiplies_on_read);
            Ok(Some(Pending::Decode(Box::new(move || {
                let mut rgba = f.data;
                if swap {
                    for p in rgba.as_chunks_mut::<4>().0 {
                        p.swap(0, 2);
                    }
                }
                if unpremultiply {
                    // The same rule on R and B, so it holds in either channel order.
                    for p in rgba.as_chunks_mut::<4>().0 {
                        *p = dxvkit_plus_enc::alpha::unpremultiply(*p);
                    }
                }
                // `straightCopy(of:)` is a new buffer: no attachments.
                let attachments = if unpremultiply { None } else { f.attachments };
                Ok(Frame { width: f.width, height: f.height, rgba, alpha, pts: Pts { value: f.pts_value, timescale: f.pts_timescale }, attachments })
            }))))
        }
    }

    /// The source rate by the one FrameTiming rule: over the container's own tables when the core's demuxer reads
    /// them, else over AVFoundation's own inputs — the platform decoder's timing, which every app's playback of such
    /// a file uses. Swift GlEnc takes AVFoundation's timing for every native source, so it converts every file
    /// AVFoundation reads; up to v0.7.1 this read 0 fps for a fragmented MP4 (empty `moov` sample tables, the
    /// samples in `moof` fragments, which DXVKit Plus reads since 0.15.0) and the encode failed ("source reports an
    /// invalid frame rate (0.0 fps)"). With the rate: the frame count of a fragmented file (its fragments' samples),
    /// `None` for any other.
    fn source_timing(path: &Path) -> (f64, Option<u64>) {
        use dxvkit_plus_core::backend::{NativeDecoder, PixelLayout};
        let movie = dxvkit_plus_core::container::Movie::open(path).ok();
        let track = movie.as_ref().and_then(|m| m.first());
        let fragmented = movie.as_ref().is_some_and(|m| m.fragments > 0);
        let frames = track.filter(|_| fragmented).map(|t| u64::from(t.frames()));
        let rate = track.map(|t| t.timing.rate).or_else(|| apple::NativeReader::open(path, PixelLayout::Bgra).ok().and_then(|r| r.timing().map(|t| t.rate)));
        (rate.map_or(0.0, |r| if r.den != 0 { r.num as f64 / r.den as f64 } else { 0.0 }), frames)
    }

    pub fn open_native(path: &Path, alpha_tag: FrameAlpha, keep_bgra: bool) -> Result<Box<dyn SourceReader>, SourceError> {
        let reader = apple::BgraReader::open(path).map_err(source_err)?;
        let (fps, fragment_frames) = source_timing(path);
        let duration = reader.duration_seconds();
        let (w, h) = (reader.width(), reader.height());
        validate_geometry(i64::from(w), i64::from(h), fps, duration)?;
        let facts = reader.alpha();
        let keeps_alpha = alpha_tag == FrameAlpha::Straight;
        // A fragmented file's AVFoundation duration can be twice its samples' (a user's capture: 7397.6 s for
        // 3698.8 s of samples), so the progress ran to about half; its fragments' count is exact. Every other file
        // keeps Swift's count.
        let swift_count = (duration * fps).round().max(0.0) as u64;
        let info = SourceInfo {
            kind: SourceKind::Native,
            fourcc: facts.codec,
            width: w,
            height: h,
            fps,
            total_frames: fragment_frames.map_or(swift_count, |n| n.min(swift_count)),
            unpremultiplies_on_read: keeps_alpha && facts.premultiplied,
        };
        Ok(Box::new(NativeSource { info, reader, alpha: alpha_tag, keep_bgra }))
    }

    pub fn read_pcm(path: &Path, rate: Option<u32>) -> Result<Option<Pcm>, String> {
        apple::read_pcm_s32(path, rate).map(|o| o.map(|p| Pcm { rate: p.rate, channels: p.channels, data: p.pcm })).map_err(|e| e.to_string())
    }

    pub fn scale(rgba: &[u8], src: (u32, u32), dst: (u32, u32), lanczos: bool) -> Result<Vec<u8>, EncodeError> {
        let f = if lanczos { apple::ScaleFilter::Lanczos } else { apple::ScaleFilter::Bilinear };
        // vImageScale_ARGB8888 treats the four channels alike, so RGBA scales as BGRA does.
        apple::scale_bgra(rgba, src.0, src.1, dst.0, dst.1, f).map_err(|e| EncodeError::Write(format!("FrameResizer: {e}")))
    }

    /// AVAssetWriter, appended in source order by the writer thread.
    struct AvSink {
        writer: Option<apple::AvWriter>,
        bgra: bool,
    }

    /// A worker's half: RGBA → BGRA in place, or nothing when the frame is the decoder's BGRA (v0.8.0; up to
    /// v0.7.2 the reader swapped to RGBA and the sink swapped back into a copy).
    struct AvMaker {
        bgra: bool,
    }

    impl Maker for AvMaker {
        fn make(&mut self, mut frame: Frame) -> Result<Made, EncodeError> {
            if !self.bgra {
                for p in frame.rgba.as_chunks_mut::<4>().0 {
                    p.swap(0, 2);
                }
            }
            Ok(Made::Picture(frame))
        }
    }

    impl crate::pipeline::Sink for AvSink {
        fn maker(&self) -> Result<Box<dyn Maker>, EncodeError> {
            Ok(Box::new(AvMaker { bgra: self.bgra }))
        }
        fn write(&mut self, made: Made) -> Result<(), EncodeError> {
            let Made::Picture(frame) = made else { return Err(EncodeError::Write("a packet for the platform writer".into())) };
            let w = self.writer.as_mut().ok_or_else(|| EncodeError::Write("writer finished".into()))?;
            w.append_with_attachments(&frame.rgba, frame.pts.value, frame.pts.timescale, frame.attachments.as_ref())
                .map_err(|e| EncodeError::Write(e.to_string()))
        }
        fn finish(mut self: Box<Self>) -> Result<Option<String>, EncodeError> {
            let w = self.writer.take().ok_or_else(|| EncodeError::Write("writer finished".into()))?;
            w.finish().map_err(|e| EncodeError::Write(e.to_string()))
        }
    }

    /// The platform's writer. `bgra`: the frames arrive in BGRA already (see [`open_native`]'s `keep_bgra`).
    pub fn av_sink(
        req: &EncodeRequest,
        codec: OutputCodec,
        w: u32,
        h: u32,
        audio: Option<Pcm>,
        bgra: bool,
    ) -> Result<Box<dyn crate::pipeline::Sink>, EncodeError> {
        let av = match codec {
            OutputCodec::ProRes(ProResVariant::ProRes422) => apple::AvCodec::ProRes422,
            OutputCodec::ProRes(ProResVariant::ProRes422Hq) => apple::AvCodec::ProRes422HQ,
            OutputCodec::ProRes(ProResVariant::ProRes422Lt) => apple::AvCodec::ProRes422LT,
            OutputCodec::ProRes(ProResVariant::ProRes422Proxy) => apple::AvCodec::ProRes422Proxy,
            OutputCodec::ProRes(ProResVariant::ProRes4444) => apple::AvCodec::ProRes4444,
            OutputCodec::H264 => apple::AvCodec::H264,
            OutputCodec::Hevc => apple::AvCodec::Hevc,
            OutputCodec::Mjpeg => apple::AvCodec::Jpeg,
            OutputCodec::Dxv(f) => return Err(EncodeError::Invalid(format!("{f:?} is not a platform codec"))),
        };
        let container = match req.container {
            OutputContainer::Mov => apple::AvContainer::Mov,
            OutputContainer::Mp4 => apple::AvContainer::Mp4,
        };
        // `CoreEncoder.makePipeline`: ProRes takes no compression properties; H.264 / HEVC the job's (profile for
        // H.264 only); Motion JPEG quality 0.85.
        let props = match codec {
            OutputCodec::H264 | OutputCodec::Hevc => {
                let v = &req.video;
                let (quality, average_bitrate) = match v.rate_control {
                    RateControl::Quality(q) => (Some(q), None),
                    RateControl::Bitrate(m) => (None, Some((m * 1_000_000.0) as i64)),
                };
                apple::CompressionProps {
                    quality,
                    average_bitrate,
                    max_keyframe_interval: (v.keyframe_interval > 0).then_some(i64::from(v.keyframe_interval)),
                    h264_profile: (codec == OutputCodec::H264).then_some(match v.h264_profile {
                        H264Profile::Baseline => apple::H264Profile::Baseline,
                        H264Profile::Main => apple::H264Profile::Main,
                        H264Profile::High => apple::H264Profile::High,
                    }),
                }
            }
            OutputCodec::Mjpeg => apple::CompressionProps { quality: Some(MJPEG_QUALITY), ..apple::CompressionProps::default() },
            _ => apple::CompressionProps::default(),
        };
        let audio = audio.map(|a| apple::PcmAudio { rate: a.rate, channels: a.channels, pcm: a.data });
        let writer = apple::AvWriter::create(&req.output, av, container, w, h, props, audio).map_err(|e| EncodeError::Write(e.to_string()))?;
        Ok(Box::new(AvSink { writer: Some(writer), bgra }))
    }
}

#[cfg(target_os = "macos")]
pub use mac::{av_sink, open_native, read_pcm, scale};

#[cfg(not(target_os = "macos"))]
mod other {
    use super::*;
    const WHY: &str = "this platform's decoder and writer arrive with the Windows backend";
    pub fn open_native(_: &Path, _: FrameAlpha, _: bool) -> Result<Box<dyn SourceReader>, SourceError> {
        Err(SourceError::ReaderStartFailed(WHY.into()))
    }
    pub fn read_pcm(_: &Path, _: Option<u32>) -> Result<Option<Pcm>, String> {
        Err(WHY.into())
    }
    pub fn scale(_: &[u8], _: (u32, u32), _: (u32, u32), _: bool) -> Result<Vec<u8>, EncodeError> {
        Err(EncodeError::Write(WHY.into()))
    }
    pub fn av_sink(_: &EncodeRequest, _: OutputCodec, _: u32, _: u32, _: Option<Pcm>, _: bool) -> Result<Box<dyn crate::pipeline::Sink>, EncodeError> {
        Err(EncodeError::Write(WHY.into()))
    }
}

#[cfg(not(target_os = "macos"))]
pub use other::{av_sink, open_native, read_pcm, scale};
