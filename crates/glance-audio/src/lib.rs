//! # glance-audio — a clip's sound on the output device, and the position the picture follows
//!
//! Swift Glance's `AudioClock` (glance/Sources/GlancePlayback/AudioClock.swift), on DXVKit Plus's
//! audio reader and cpal:
//! - the clip's **first audio track** is decoded whole into memory (Swift: one `AVAudioPCMBuffer`;
//!   sample-accurate seek and loop by slicing) — DXVKit Plus serves the same frames AVFoundation's
//!   `AVAssetReader` serves (edit list, priming, empty edits as silence);
//! - the format is checked at the trust boundary ([`validate_format`]: 1 to 64 channels, a finite
//!   rate above 0); anything else is a typed error, never trusted;
//! - it plays on the default output device **at the device's own rate and channel count** (the
//!   device is never switched to the clip's rate): the track is resampled once at open with
//!   rubato's FFT resampler ([`resample`]; Swift: `AVAudioEngine`'s mixer resamples), and channels
//!   are mapped in the device callback ([`map_frame`]);
//! - volume (0…1) and mute are a gain in the callback ([`effective_gain`]); they never stop,
//!   reschedule or re-anchor anything, so the position runs on at gain 0 (Swift's Stage D rule);
//! - the position ([`AudioClock::position`]) is the movie time of the sound **heard now**: the frame
//!   the device callback started with, at the time CoreAudio says it reaches the speaker
//!   ([`heard_nanos`]), advanced by the stream clock; looping wraps it at the audio's own length,
//!   otherwise it holds the last frame (Swift's `wrappedPositionSeconds`, [`wrap_frame`]). Until the
//!   first frame after a start or seek is heard there is no position (`None`) — the caller holds the
//!   frame, and falls back to the wall clock after 0.25 s (glance-playback).
//!
//! The output device can change under a playing clip: the user picks another output, or the
//! device's own rate changes (Audio MIDI Setup, another app). The clock follows it
//! ([`AudioClock::follow_device`]): when cpal reports the route changed or the stream invalid, or
//! the default output's rate or channel count differs from the stream's, the stream is rebuilt on
//! the default output at its new rate, the track re-read and resampled once to that rate, and play
//! resumes at the position heard — so the speed stays right (C3; Swift's `AVAudioEngine` re-runs
//! its graph on `AVAudioEngineConfigurationChange`). Buffer under- and overruns are counted, never
//! a failure.
//!
//! An undecodable or malformed track is an [`AudioError`] that the player maps to Swift's
//! `.failed(reason:)` ([`AudioError::state`]) while the picture plays on the wall clock.
#![forbid(unsafe_code)]

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use glance_media::{AudioInfo, AudioReader, AudioTiming, MediaError, MediaTime, TICKS_PER_SECOND, Tick};
use rubato::Resampler;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::SeqCst};
use std::sync::{Arc, Mutex};

/// The most channels a track may state (Swift's §3 bound).
pub const MAX_CHANNELS: i64 = 64;

/// What the clip's sound is doing (Swift's `DXVPlayer.AudioState`), for the viewer to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioState {
    /// The track plays and drives the clock.
    Playing { sample_rate: u32, channels: u32 },
    /// No audio track: the picture plays on the wall clock.
    None { reason: String },
    /// A track exists but does not play (unreadable, malformed, no device, or it stopped): the
    /// picture plays on the wall clock.
    Failed { reason: String },
}

/// Why a clip's sound does not play (Swift's `AudioClockError`, same words).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioError {
    /// The container has no audio track. Not a failure: the clip plays silently.
    NoAudioTrack,
    /// The player was asked to play without sound (measurements). Not a failure.
    Off,
    /// The format failed the trust-boundary check.
    InvalidFormat(String),
    /// The track (or the output device) could not be read, decoded or started.
    Unreadable(String),
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioError::NoAudioTrack => f.write_str("no audio track"),
            AudioError::Off => f.write_str("sound off"),
            AudioError::InvalidFormat(r) => write!(f, "invalid audio format: {r}"),
            AudioError::Unreadable(r) => write!(f, "audio unreadable: {r}"),
        }
    }
}

impl std::error::Error for AudioError {}

impl AudioError {
    /// The state the viewer shows: `None` for a clip without audio, `Failed` with the reason else.
    pub fn state(&self) -> AudioState {
        match self {
            AudioError::NoAudioTrack | AudioError::Off => AudioState::None { reason: self.to_string() },
            _ => AudioState::Failed { reason: self.to_string() },
        }
    }
}

impl From<MediaError> for AudioError {
    fn from(e: MediaError) -> Self {
        AudioError::Unreadable(e.to_string())
    }
}

/// Swift's §3 check (`AudioClock.validateAudioFormat`): the rate finite and above 0, 1 to 64
/// channels.
pub fn validate_format(sample_rate: f64, channels: i64) -> Result<(), AudioError> {
    if !sample_rate.is_finite() || sample_rate <= 0.0 {
        return Err(AudioError::InvalidFormat(format!("non-finite or zero sample rate ({sample_rate})")));
    }
    if !(1..=MAX_CHANNELS).contains(&channels) {
        return Err(AudioError::InvalidFormat(format!("channel count {channels} out of [1,64]")));
    }
    Ok(())
}

/// Swift's gain contract (`AudioClock.effectiveGain`): muted is 0, else the volume clamped to 0…1
/// (a NaN volume is 0).
pub fn effective_gain(volume: f32, muted: bool) -> f32 {
    if muted || volume.is_nan() { 0.0 } else { volume.clamp(0.0, 1.0) }
}

/// A whole audio track, decoded: interleaved `f32` frames at the track's own rate.
#[derive(Clone, Debug)]
pub struct Pcm {
    pub samples: Vec<f32>,
    pub channels: u32,
    pub rate: u32,
    /// Every output frame's real start on the movie timeline (DXVKit Plus).
    pub timing: AudioTiming,
}

impl Pcm {
    pub fn frames(&self) -> u64 {
        (self.samples.len() / (self.channels.max(1) as usize)) as u64
    }
}

/// Decode a whole track (Swift's `decodeFullPCM`), checking the format first.
pub fn decode_track(reader: &mut AudioReader) -> Result<Pcm, AudioError> {
    let (rate, channels) = (reader.rate(), reader.channels());
    validate_format(f64::from(rate), i64::from(channels))?;
    let mut samples = Vec::with_capacity((reader.timing().frame_count() as usize).saturating_mul(channels as usize));
    while let Some(block) = reader.read(16_384)? {
        if block.channels != channels || block.rate != rate {
            return Err(AudioError::InvalidFormat(format!("a block of {} ch at {} Hz in a {channels} ch, {rate} Hz track", block.channels, block.rate)));
        }
        samples.extend_from_slice(&block.samples);
    }
    if samples.is_empty() {
        return Err(AudioError::Unreadable("no audio samples decoded".into()));
    }
    Ok(Pcm { samples, channels, rate, timing: reader.timing().clone() })
}

/// Resample interleaved frames from `from` Hz to `to` Hz with rubato's FFT resampler (fixed ratio,
/// the whole clip at once, its delay trimmed so output frame 0 is input frame 0). Equal rates pass
/// the samples through untouched.
pub fn resample(samples: Vec<f32>, channels: u32, from: u32, to: u32) -> Result<Vec<f32>, AudioError> {
    if from == to {
        return Ok(samples);
    }
    let ch = channels.max(1) as usize;
    let frames = samples.len() / ch;
    let err = |e: &dyn std::fmt::Display| AudioError::Unreadable(format!("resampling {from} Hz to {to} Hz: {e}"));
    let mut r = rubato::Fft::<f32>::new(from as usize, to as usize, 1024, ch, rubato::FixedSync::Both).map_err(|e| err(&e))?;
    let input = rubato::audioadapter_buffers::direct::InterleavedSlice::new(&samples, ch, frames).map_err(|e| err(&e))?;
    let need = r.process_all_needed_output_len(frames);
    let mut out = vec![0.0f32; need.saturating_mul(ch)];
    let produced = {
        let mut output = rubato::audioadapter_buffers::direct::InterleavedSlice::new_mut(&mut out, ch, need).map_err(|e| err(&e))?;
        r.process_all_into_buffer(&input, &mut output, frames, None).map_err(|e| err(&e))?.1
    };
    out.truncate(produced.saturating_mul(ch));
    Ok(out)
}

/// One frame of the track to one frame of the device, times `gain`: equal counts channel for
/// channel; mono to every device channel; several to a mono device as their mean; otherwise
/// channel k to channel k (missing ones silent, extra ones dropped).
pub fn map_frame(src: &[f32], dst: &mut [f32], gain: f32) {
    if src.len() == dst.len() {
        dst.iter_mut().zip(src).for_each(|(d, s)| *d = s * gain);
    } else if let [mono] = src {
        dst.iter_mut().for_each(|d| *d = mono * gain);
    } else if let [out] = dst {
        *out = src.iter().sum::<f32>() / src.len().max(1) as f32 * gain;
    } else {
        dst.iter_mut().enumerate().for_each(|(k, d)| *d = src.get(k).copied().unwrap_or(0.0) * gain);
    }
}

/// Swift's `wrappedPositionSeconds` in frames: an unwrapped frame wrapped into `[0, total)` when
/// looping, held at the last frame otherwise; `None` for an empty track.
pub fn wrap_frame(frame: i128, total: u64, loops: bool) -> Option<u64> {
    let total = i128::from(total);
    if total <= 0 {
        return None;
    }
    let f = if loops { frame.rem_euclid(total) } else { frame.clamp(0, total.saturating_sub(1)) };
    u64::try_from(f).ok()
}

/// When the first frame of a device callback is heard, in stream-clock nanoseconds. cpal's
/// `playback` instant is the callback's host time plus the device buffer, the device latency and
/// the safety offset. On CoreAudio that host time already is the moment the device consumes the
/// buffer's first frame (Apple's Core Audio engineers on coreaudio-api; the HAL's own formula puts
/// the safety offset and the buffer before it), so the buffer is taken back off here; what remains
/// added is the device latency (correct) and the safety offset (a few frames early), and the stream
/// latency, which cpal does not read, is missing (`docs/handoffs/c2-time-and-sound.md`, DIAGNOSE 2).
pub fn heard_nanos(info: &cpal::OutputCallbackInfo, frames: usize, rate: u32) -> u64 {
    let ts = info.timestamp();
    let playback = ts.playback.saturating_duration_since(cpal::StreamInstant::ZERO).as_nanos();
    let buffer = if cfg!(any(target_os = "macos", target_os = "ios")) { (frames as u128).saturating_mul(1_000_000_000) / u128::from(rate.max(1)) } else { 0 };
    u64::try_from(playback.saturating_sub(buffer)).unwrap_or(u64::MAX)
}

/// What a cpal stream error means for the clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamError {
    /// A buffer under- or overrun: counted, the sound plays on.
    Overrun,
    /// The default output moved (cpal's DefaultOutput unit follows it), the stream's format no
    /// longer holds, or its device went away: rebuilt on the default output.
    Reroute,
    /// Anything else: the sound is lost (the picture falls back to the wall clock).
    Fatal,
}

/// cpal's error kinds as the clock reads them.
pub fn stream_error(kind: cpal::ErrorKind) -> StreamError {
    match kind {
        cpal::ErrorKind::Xrun => StreamError::Overrun,
        cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::StreamInvalidated | cpal::ErrorKind::DeviceNotAvailable => StreamError::Reroute,
        _ => StreamError::Fatal,
    }
}

/// No seek waiting for the callback.
const NO_SEEK: u64 = u64::MAX;

/// State shared with the device callback: atomics only, no lock and no allocation in the callback.
struct Shared {
    /// The track at the device's rate, interleaved, the track's channels.
    samples: Vec<f32>,
    channels: usize,
    /// Frames in `samples`.
    len: u64,
    /// The next frame the callback plays.
    cursor: AtomicU64,
    playing: AtomicBool,
    loops: AtomicBool,
    gain: AtomicU32,
    /// A start / seek for the callback to apply at its next buffer.
    pending: AtomicU64,
    /// Set by a start / seek, cleared by the callback once it has applied it: no position until then.
    unprimed: AtomicBool,
    failed: AtomicBool,
    error: Mutex<Option<String>>,
    /// cpal reported the route changed or the stream invalid: rebuild on the default output.
    reroute: AtomicBool,
    /// Buffer under- / overruns cpal reported (never a failure).
    xruns: AtomicU64,
    /// Frames the callback has played (all buffers while playing): the speed measure.
    rendered: AtomicU64,
    /// Seqlock over the anchor (the callback writes, the clock reads).
    seq: AtomicU64,
    /// The frame a playing callback started with, and when it is heard.
    anchor_frame: AtomicU64,
    anchor_nanos: AtomicU64,
    /// When the first frame after the last start / seek is heard.
    primed_nanos: AtomicU64,
    /// cpal's playback minus callback instant of the last callback, and its buffer, for the record.
    reported_delay_nanos: AtomicU64,
    buffer_frames: AtomicU64,
}

impl Shared {
    fn render(&self, out: &mut [f32], out_channels: usize, heard: u64) {
        let frames = out.len() / out_channels.max(1);
        let seek = self.pending.swap(NO_SEEK, SeqCst);
        if seek != NO_SEEK {
            self.cursor.store(seek, SeqCst);
        }
        self.buffer_frames.store(frames as u64, SeqCst);
        if !self.playing.load(SeqCst) {
            out.fill(0.0);
            return;
        }
        let gain = f32::from_bits(self.gain.load(SeqCst));
        let loops = self.loops.load(SeqCst);
        let start = self.cursor.load(SeqCst);
        let mut cursor = start;
        for dst in out.chunks_exact_mut(out_channels.max(1)) {
            if cursor >= self.len && loops {
                cursor = 0;
            }
            let at = (cursor as usize).saturating_mul(self.channels);
            match self.samples.get(at..at.saturating_add(self.channels)) {
                Some(src) if cursor < self.len => {
                    map_frame(src, dst, gain);
                    cursor = cursor.saturating_add(1);
                }
                _ => dst.fill(0.0),
            }
        }
        self.cursor.store(cursor, SeqCst);
        self.rendered.fetch_add(frames as u64, SeqCst);
        self.seq.fetch_add(1, SeqCst);
        self.anchor_frame.store(start, SeqCst);
        self.anchor_nanos.store(heard, SeqCst);
        if seek != NO_SEEK {
            self.primed_nanos.store(heard, SeqCst);
        }
        self.seq.fetch_add(1, SeqCst);
        if seek != NO_SEEK {
            self.unprimed.store(false, SeqCst);
        }
    }

    /// (anchor frame, its heard time, primed time), consistent.
    fn anchor(&self) -> (u64, u64, u64) {
        loop {
            let a = self.seq.load(SeqCst);
            let v = (self.anchor_frame.load(SeqCst), self.anchor_nanos.load(SeqCst), self.primed_nanos.load(SeqCst));
            if a.is_multiple_of(2) && self.seq.load(SeqCst) == a {
                return v;
            }
            std::hint::spin_loop();
        }
    }
}

/// The output device's format: its own rate and channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceFormat {
    pub rate: u32,
    pub channels: u16,
}

/// The default output device and its own format, when there is one.
pub fn default_output() -> Result<(cpal::Device, DeviceFormat), AudioError> {
    let device = cpal::default_host().default_output_device().ok_or_else(|| AudioError::Unreadable("no audio output device".into()))?;
    let config = device.default_output_config().map_err(|e| AudioError::Unreadable(format!("output device: {e}")))?;
    Ok((device, DeviceFormat { rate: config.sample_rate(), channels: config.channels() }))
}

/// Reads the track again for a rebuilt stream (the decoded track is not kept beside its
/// resampled copy).
pub type Reread = Box<dyn Fn() -> Result<Pcm, AudioError> + Send>;

/// How often the default output's format is compared with the stream's.
pub const DEVICE_POLL: std::time::Duration = std::time::Duration::from_millis(250);

/// A stream rebuilt on another device format ([`AudioClock::follow_device`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reroute {
    pub from: DeviceFormat,
    pub to: DeviceFormat,
    /// What triggered it: cpal's report, or the polled format.
    pub why: String,
    /// Re-reading, resampling and restarting took this long.
    pub took: std::time::Duration,
}

/// A clip's first audio track playing on the output device: the audio-master clock.
pub struct AudioClock {
    stream: cpal::Stream,
    shared: Arc<Shared>,
    device: DeviceFormat,
    track: TrackFormat,
    timing: AudioTiming,
    playing: bool,
    /// The frame (device rate) the clock holds while paused, and where play resumes.
    paused_at: u64,
    volume: f32,
    muted: bool,
    reread: Reread,
    last_poll: std::time::Instant,
    reroutes: u32,
}

impl std::fmt::Debug for AudioClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioClock").field("device", &self.device).field("track", &self.track).field("playing", &self.playing).finish()
    }
}

/// The track as the file states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackFormat {
    pub rate: u32,
    pub channels: u32,
    /// Output frames at the track's rate (the loop length).
    pub frames: u64,
}

impl AudioClock {
    /// The clip's first audio track (`tracks` is `MediaInfo::audio`) on the default output device,
    /// paused at frame 0. No track: [`AudioError::NoAudioTrack`].
    pub fn open(path: &Path, tracks: &[AudioInfo], loops: bool) -> Result<AudioClock, AudioError> {
        if tracks.is_empty() {
            return Err(AudioError::NoAudioTrack);
        }
        let path = path.to_path_buf();
        let reread: Reread = Box::new(move || decode_track(&mut glance_media::open_audio(&path, 0)?));
        let pcm = reread()?;
        let (device, format) = default_output()?;
        Self::start(pcm, &device, format, loops, reread)
    }

    /// A decoded track on `device` (`format` is the device's own); `reread` gives the track again
    /// when the stream is rebuilt on another device format.
    pub fn start(pcm: Pcm, device: &cpal::Device, format: DeviceFormat, loops: bool, reread: Reread) -> Result<AudioClock, AudioError> {
        let track = TrackFormat { rate: pcm.rate, channels: pcm.channels, frames: pcm.frames() };
        let timing = pcm.timing.clone();
        let (stream, shared) = Self::stream(pcm, device, format, loops)?;
        Ok(AudioClock {
            stream,
            shared,
            device: format,
            track,
            timing,
            playing: false,
            paused_at: 0,
            volume: 1.0,
            muted: false,
            reread,
            last_poll: std::time::Instant::now(),
            reroutes: 0,
        })
    }

    /// The track resampled to `format`'s rate and a running (silent until played) stream on
    /// `device` that plays it.
    fn stream(pcm: Pcm, device: &cpal::Device, format: DeviceFormat, loops: bool) -> Result<(cpal::Stream, Arc<Shared>), AudioError> {
        validate_format(f64::from(format.rate), i64::from(format.channels))?;
        let samples = resample(pcm.samples, pcm.channels, pcm.rate, format.rate)?;
        let channels = pcm.channels as usize;
        let len = (samples.len() / channels.max(1)) as u64;
        let shared = Arc::new(Shared {
            samples,
            channels,
            len,
            cursor: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            loops: AtomicBool::new(loops),
            gain: AtomicU32::new(1.0f32.to_bits()),
            pending: AtomicU64::new(NO_SEEK),
            unprimed: AtomicBool::new(true),
            failed: AtomicBool::new(false),
            error: Mutex::new(None),
            reroute: AtomicBool::new(false),
            xruns: AtomicU64::new(0),
            rendered: AtomicU64::new(0),
            seq: AtomicU64::new(0),
            anchor_frame: AtomicU64::new(0),
            anchor_nanos: AtomicU64::new(0),
            primed_nanos: AtomicU64::new(u64::MAX),
            reported_delay_nanos: AtomicU64::new(0),
            buffer_frames: AtomicU64::new(0),
        });
        let config = cpal::StreamConfig { channels: format.channels, sample_rate: format.rate, buffer_size: cpal::BufferSize::Default };
        let (render, failure) = (shared.clone(), shared.clone());
        let out_channels = usize::from(format.channels);
        let rate = format.rate;
        let stream = device
            .build_output_stream::<f32, _, _>(
                config,
                move |out: &mut [f32], info: &cpal::OutputCallbackInfo| {
                    let frames = out.len() / out_channels.max(1);
                    let ts = info.timestamp();
                    let delay = ts.playback.saturating_duration_since(ts.callback).as_nanos();
                    render.reported_delay_nanos.store(u64::try_from(delay).unwrap_or(u64::MAX), SeqCst);
                    render.render(out, out_channels, heard_nanos(info, frames, rate));
                },
                move |e: cpal::Error| {
                    let what = stream_error(e.kind());
                    if what == StreamError::Overrun {
                        failure.xruns.fetch_add(1, SeqCst);
                        return;
                    }
                    if let Ok(mut slot) = failure.error.lock() {
                        *slot = Some(e.to_string());
                    }
                    match what {
                        StreamError::Reroute => failure.reroute.store(true, SeqCst),
                        _ => failure.failed.store(true, SeqCst),
                    }
                },
                None,
            )
            .map_err(|e| AudioError::Unreadable(format!("output stream: {e}")))?;
        stream.play().map_err(|e| AudioError::Unreadable(format!("engine start failed: {e}")))?;
        Ok((stream, shared))
    }

    /// Follow the output device: when cpal reported the route changed or the stream invalid, or
    /// (checked every [`DEVICE_POLL`]) the default output's rate or channel count is not the
    /// stream's, rebuild the stream on the default output at its own format, the track re-read and
    /// resampled once, at the position heard, playing or paused as it was, with the same volume,
    /// mute and loop. `Ok(None)`: nothing changed. An error means no sound is left (the caller
    /// falls back to the wall clock).
    pub fn follow_device(&mut self) -> Result<Option<Reroute>, AudioError> {
        let reported = self.shared.reroute.swap(false, SeqCst);
        let now = std::time::Instant::now();
        if !reported && now.saturating_duration_since(self.last_poll) < DEVICE_POLL {
            return Ok(None);
        }
        self.last_poll = now;
        let (device, format) = default_output()?;
        if !reported && format == self.device {
            return Ok(None);
        }
        let why = if reported {
            self.shared.error.lock().ok().and_then(|e| e.clone()).unwrap_or_else(|| "the route changed".into())
        } else {
            format!("the default output is now {} Hz, {} ch", format.rate, format.channels)
        };
        let started = std::time::Instant::now();
        // Where the sound is (in track frames), before the old stream goes.
        let at = self.to_source(self.position_frame().unwrap_or(self.paused_at));
        let was_playing = self.playing;
        let pcm = (self.reread)()?;
        let (stream, shared) = Self::stream(pcm, &device, format, self.shared.loops.load(SeqCst))?;
        let from = self.device;
        self.stream = stream;
        self.shared = shared;
        self.device = format;
        self.paused_at = self.to_device(at).min(self.shared.len.saturating_sub(1));
        self.playing = false;
        self.apply_gain();
        if was_playing {
            self.play();
        }
        self.reroutes = self.reroutes.saturating_add(1);
        Ok(Some(Reroute { from, to: format, why, took: started.elapsed() }))
    }

    /// Streams rebuilt so far, buffer under- / overruns cpal reported on the current stream, and
    /// the frames its callback has played.
    pub fn device_counts(&self) -> (u32, u64, u64) {
        (self.reroutes, self.shared.xruns.load(SeqCst), self.shared.rendered.load(SeqCst))
    }

    /// What the viewer shows while this clock plays.
    pub fn state(&self) -> AudioState {
        AudioState::Playing { sample_rate: self.track.rate, channels: self.track.channels }
    }

    pub fn track(&self) -> TrackFormat {
        self.track
    }

    pub fn device(&self) -> DeviceFormat {
        self.device
    }

    /// The loop length in seconds.
    pub fn duration_seconds(&self) -> f64 {
        self.track.frames as f64 / f64::from(self.track.rate.max(1))
    }

    /// Start or resume from where the clock holds; `false` when the device failed (the caller
    /// falls back to the wall clock).
    pub fn play(&mut self) -> bool {
        if self.failed().is_some() {
            return false;
        }
        self.shared.unprimed.store(true, SeqCst);
        self.shared.pending.store(self.paused_at, SeqCst);
        self.shared.playing.store(true, SeqCst);
        self.playing = true;
        true
    }

    /// Hold the position where it is heard now.
    pub fn pause(&mut self) {
        if let Some(at) = self.position_frame() {
            self.paused_at = at;
        }
        self.shared.playing.store(false, SeqCst);
        self.playing = false;
    }

    /// Continue from movie time `t` (the video frame's real start). While playing the device
    /// moves there at its next buffer; while paused play resumes there.
    pub fn seek(&mut self, t: MediaTime) {
        let source = self.timing.frame_at(t).min(self.track.frames.saturating_sub(1));
        let frame = self.to_device(source).min(self.shared.len.saturating_sub(1));
        self.paused_at = frame;
        if self.playing {
            self.shared.unprimed.store(true, SeqCst);
            self.shared.pending.store(frame, SeqCst);
        }
    }

    /// Loop at the audio's own length, or hold its last frame (kept in lockstep with the video).
    pub fn set_loops(&mut self, loops: bool) {
        self.shared.loops.store(loops, SeqCst);
    }

    /// Output level, linear, clamped to 0…1. Never moves the clock.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        self.apply_gain();
    }

    /// Mute, separate from the volume so unmute restores it. Never moves the clock.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.apply_gain();
    }

    fn apply_gain(&self) {
        self.shared.gain.store(effective_gain(self.volume, self.muted).to_bits(), SeqCst);
    }

    /// Why the device stopped, when it did.
    pub fn failed(&self) -> Option<String> {
        if !self.shared.failed.load(SeqCst) {
            return None;
        }
        Some(self.shared.error.lock().ok().and_then(|e| e.clone()).unwrap_or_else(|| "the output device failed".into()))
    }

    fn to_device(&self, source: u64) -> u64 {
        let (d, s) = (u128::from(self.device.rate), u128::from(self.track.rate.max(1)));
        u64::try_from((u128::from(source) * d + s / 2) / s).unwrap_or(u64::MAX)
    }

    fn to_source(&self, device: u64) -> u64 {
        let (d, s) = (u128::from(self.device.rate.max(1)), u128::from(self.track.rate));
        u64::try_from(u128::from(device) * s / d).unwrap_or(u64::MAX).min(self.track.frames.saturating_sub(1))
    }

    /// The device-rate frame heard now, wrapped or held as the loop setting says; `None` until the
    /// first frame after a start or seek is heard, or after the device failed.
    pub fn position_frame(&self) -> Option<u64> {
        self.position_frame_at(std::time::Instant::now())
    }

    /// [`AudioClock::position_frame`] at host instant `at` (the moment a picture will be shown,
    /// say): the stream clock now, moved by `at - now`.
    pub fn position_frame_at(&self, at: std::time::Instant) -> Option<u64> {
        if self.shared.failed.load(SeqCst) {
            return None;
        }
        if !self.playing {
            return Some(self.paused_at);
        }
        if self.shared.unprimed.load(SeqCst) {
            return None;
        }
        let (frame, heard, primed) = self.shared.anchor();
        let host_now = std::time::Instant::now();
        let shift = i128::try_from(at.saturating_duration_since(host_now).as_nanos()).unwrap_or(0)
            - i128::try_from(host_now.saturating_duration_since(at).as_nanos()).unwrap_or(0);
        let stream_now = i128::try_from(self.stream.now().saturating_duration_since(cpal::StreamInstant::ZERO).as_nanos()).unwrap_or(i128::MAX);
        let now = u64::try_from((stream_now + shift).max(0)).unwrap_or(u64::MAX);
        if now < primed {
            return None;
        }
        let elapsed = i128::from(now) - i128::from(heard);
        let advanced = elapsed * i128::from(self.device.rate) / 1_000_000_000;
        wrap_frame(i128::from(frame) + advanced, self.shared.len, self.shared.loops.load(SeqCst))
    }

    /// A clip that does not loop has played its last audio frame.
    pub fn ended(&self) -> bool {
        !self.shared.loops.load(SeqCst) && self.playing && self.position_frame().is_some_and(|f| f.saturating_add(1) >= self.shared.len)
    }

    /// Where the sound's loop ends on the movie timeline (its last frame's start plus one frame).
    pub fn end_ticks(&self) -> Option<Tick> {
        let last = self.timing.position_of(self.track.frames.checked_sub(1)?)?;
        let t = ticks(last);
        Some(Tick(t.0.saturating_add(TICKS_PER_SECOND / i64::from(self.track.rate.max(1)))))
    }

    /// The movie time of the sound heard now (Swift's `currentSeconds`, exact through the edit
    /// list: DXVKit Plus's `AudioTiming::position_of`). `None` until primed.
    pub fn position(&self) -> Option<MediaTime> {
        self.position_at(std::time::Instant::now())
    }

    /// The movie time heard at host instant `at`.
    pub fn position_at(&self, at: std::time::Instant) -> Option<MediaTime> {
        let source = self.to_source(self.position_frame_at(at)?);
        self.timing.position_of(source)
    }

    /// [`AudioClock::position_at`] in transport ticks.
    pub fn position_ticks_at(&self, at: std::time::Instant) -> Option<Tick> {
        self.position_at(at).map(ticks)
    }

    /// cpal's reported delay (playback minus callback instant) and the callback's buffer, in
    /// nanoseconds and frames: what the heard time is built from.
    pub fn reported_latency(&self) -> (u64, u64) {
        (self.shared.reported_delay_nanos.load(SeqCst), self.shared.buffer_frames.load(SeqCst))
    }
}

/// A media time in transport ticks (floor).
pub fn ticks(t: MediaTime) -> Tick {
    let v = i128::from(t.value) * i128::from(TICKS_PER_SECOND) / i128::from(t.timescale.max(1));
    Tick(i64::try_from(v).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn malformed_formats_are_rejected_with_swifts_words() {
        assert_eq!(validate_format(48_000.0, 2), Ok(()));
        assert_eq!(validate_format(8_000.0, 64), Ok(()));
        for (rate, ch, why) in [
            (0.0, 2, "non-finite or zero sample rate (0)"),
            (-1.0, 2, "non-finite or zero sample rate (-1)"),
            (f64::NAN, 2, "non-finite or zero sample rate (NaN)"),
            (f64::INFINITY, 2, "non-finite or zero sample rate (inf)"),
            (48_000.0, 0, "channel count 0 out of [1,64]"),
            (48_000.0, 65, "channel count 65 out of [1,64]"),
        ] {
            let e = validate_format(rate, ch).unwrap_err();
            assert_eq!(e, AudioError::InvalidFormat(why.into()));
            assert_eq!(e.state(), AudioState::Failed { reason: format!("invalid audio format: {why}") });
        }
        assert_eq!(AudioError::NoAudioTrack.state(), AudioState::None { reason: "no audio track".into() });
    }

    #[test]
    fn an_overrun_plays_on_a_moved_device_reroutes_anything_else_is_lost() {
        // C2 treated every cpal error as fatal, an xrun included.
        assert_eq!(stream_error(cpal::ErrorKind::Xrun), StreamError::Overrun);
        for k in [cpal::ErrorKind::DeviceChanged, cpal::ErrorKind::StreamInvalidated, cpal::ErrorKind::DeviceNotAvailable] {
            assert_eq!(stream_error(k), StreamError::Reroute, "{k:?}");
        }
        for k in [cpal::ErrorKind::BackendError, cpal::ErrorKind::PermissionDenied, cpal::ErrorKind::HostUnavailable] {
            assert_eq!(stream_error(k), StreamError::Fatal, "{k:?}");
        }
    }

    #[test]
    fn gain_is_volume_clamped_and_mute_is_zero() {
        assert_eq!(effective_gain(0.5, false), 0.5);
        assert_eq!(effective_gain(1.7, false), 1.0);
        assert_eq!(effective_gain(-0.2, false), 0.0);
        assert_eq!(effective_gain(f32::NAN, false), 0.0);
        assert_eq!(effective_gain(0.8, true), 0.0);
    }

    #[test]
    fn channels_map_to_the_device() {
        let mut two = [0.0; 2];
        map_frame(&[0.5], &mut two, 1.0);
        assert_eq!(two, [0.5, 0.5]);
        let mut one = [0.0; 1];
        map_frame(&[0.2, 0.6], &mut one, 0.5);
        assert!((one[0] - 0.2).abs() < 1e-6);
        let mut two = [0.0; 2];
        map_frame(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6], &mut two, 1.0);
        assert_eq!(two, [0.1, 0.2]);
        let mut four = [9.0; 4];
        map_frame(&[0.1, 0.2], &mut four, 1.0);
        assert_eq!(four, [0.1, 0.2, 0.0, 0.0]);
    }

    #[test]
    fn the_position_wraps_when_looping_and_holds_otherwise() {
        assert_eq!(wrap_frame(10, 100, true), Some(10));
        assert_eq!(wrap_frame(250, 100, true), Some(50));
        assert_eq!(wrap_frame(250, 100, false), Some(99));
        assert_eq!(wrap_frame(-1, 100, false), Some(0));
        assert_eq!(wrap_frame(5, 0, true), None);
    }

    #[test]
    fn resampling_keeps_length_and_timing() {
        // A 1 kHz sine at 44.1 kHz, one second, to 48 kHz: 48 000 frames, and the tone keeps its
        // time (rubato trims its delay in whole frames): the phase measured over the middle half is
        // the input's within one output frame (21 us; measured 0.54 frames, 11 us). Equal rates
        // pass through untouched.
        let w = 2.0 * std::f64::consts::PI * 1000.0;
        let tone: Vec<f32> = (0..44_100).map(|i| (w * f64::from(i) / 44_100.0).sin() as f32).collect();
        let out = resample(tone.clone(), 1, 44_100, 48_000).unwrap();
        assert!((out.len() as i64 - 48_000).abs() <= 1, "{}", out.len());
        let (mut s, mut c) = (0.0f64, 0.0f64);
        for i in 12_000..36_000 {
            let t = w * f64::from(i) / 48_000.0;
            s += f64::from(out[i as usize]) * t.sin();
            c += f64::from(out[i as usize]) * t.cos();
        }
        let shift_frames = c.atan2(s) / (w / 48_000.0);
        eprintln!("44.1 -> 48 kHz: {} frames, tone shifted {shift_frames:.4} output frames", out.len());
        assert!(shift_frames.abs() < 1.0, "{shift_frames}");
        assert_eq!(resample(tone.clone(), 1, 44_100, 44_100).unwrap(), tone);
    }
}
