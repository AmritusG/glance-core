//! Frames an app makes itself (a render, a reel), written to a movie by the engine's own platform writer (v0.9.1): the
//! same AVAssetWriter path, compression properties and RGBA → BGRA step an encode job uses, without a source file.
//! Platform codecs only (H.264, HEVC, ProRes, Motion JPEG), macOS only for now (elsewhere it says why).

use crate::EncodeError;
use crate::audio::AudioPlan;
use crate::codec::{OutputCodec, OutputContainer, VideoSettings};
use crate::frame::{Frame, FrameAlpha, Pts};
use crate::pipeline::{EncodeRequest, Maker, Sink};
use crate::size::{AspectMode, OutputSize, ResizeQuality};
use crate::source::AlphaType;
use std::path::Path;

/// A movie being written, one frame at a time, at a fixed rate.
pub struct FrameWriter {
    sink: Box<dyn Sink>,
    maker: Box<dyn Maker>,
    size: (u32, u32),
    fps: u32,
    frames: u64,
}

impl FrameWriter {
    /// A movie of `size` pixels at `fps` frames a second, at `output` (an existing file is replaced by the writer).
    pub fn create(
        output: &Path,
        codec: OutputCodec,
        container: OutputContainer,
        size: (u32, u32),
        fps: u32,
        video: VideoSettings,
    ) -> Result<FrameWriter, EncodeError> {
        if codec.dxv_format().is_some() {
            return Err(EncodeError::Invalid(format!("{codec:?} is not a platform codec: FrameWriter writes the platform's codecs")));
        }
        if size.0 == 0 || size.1 == 0 || fps == 0 {
            return Err(EncodeError::Invalid(format!("a {} x {} movie at {fps} fps", size.0, size.1)));
        }
        let req = EncodeRequest {
            source: output.to_path_buf(),
            output: output.to_path_buf(),
            codec,
            container,
            video,
            output_size: OutputSize::Custom { width: size.0, height: size.1 },
            resize_quality: ResizeQuality::Auto,
            aspect: AspectMode::Letterbox,
            crop: None,
            frames: None,
            writer_version: String::new(),
            hap_chunks: 1,
            alpha_type: AlphaType::Premultiplied,
            audio: AudioPlan { enabled: false, ..AudioPlan::default() },
            refined_bc4: false,
        };
        let sink = crate::platform::av_sink(&req, codec, size.0, size.1, None, false)?;
        let maker = sink.maker()?;
        Ok(FrameWriter { sink, maker, size, fps, frames: 0 })
    }

    /// The next frame: tightly packed RGBA8 of the movie's size, top row first.
    pub fn push(&mut self, rgba: Vec<u8>) -> Result<(), EncodeError> {
        let want = self.size.0 as usize * self.size.1 as usize * 4;
        if rgba.len() != want {
            return Err(EncodeError::Invalid(format!("a frame of {} bytes for a {} x {} movie ({want})", rgba.len(), self.size.0, self.size.1)));
        }
        let pts = Pts { value: i64::try_from(self.frames).unwrap_or(i64::MAX), timescale: i32::try_from(self.fps).unwrap_or(i32::MAX) };
        let frame = Frame { width: self.size.0, height: self.size.1, rgba, alpha: FrameAlpha::Straight, pts, attachments: None };
        let made = self.maker.make(frame)?;
        self.sink.write(made)?;
        self.frames += 1;
        Ok(())
    }

    /// Frames written so far.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Close the movie; returns the frames written.
    pub fn finish(self) -> Result<u64, EncodeError> {
        self.sink.finish()?;
        Ok(self.frames)
    }
}
