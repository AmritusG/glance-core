//! # glance-playback — a clip on a clock, and the numbers that say how it played
//!
//! [`Clock`] is Swift Glance's `FrameClock` with its players' transport rules
//! (glance/Sources/GlancePlayback/FrameClock.swift, DXVPlayer.swift, HAPPlayer.swift, and
//! Glance/PlayerModel.swift for restart and scrub), on DXVKit Plus's exact transport:
//! - **audio master** when the clip's sound plays: the frame on screen is the one whose start is
//!   nearest the movie time being heard (`Clip::nearest_frame`: Swift's `round(seconds x fps)` on
//!   a grid, the nearest real start on a table), clamped to the clip — audio loops at its own
//!   length and the picture holds its last frame while a longer track plays on;
//! - if the audio position stays unavailable for more than [`AUDIO_PRIME_GRACE`] (0.25 s) after
//!   play, or the device fails, the clock falls back to the wall clock, re-anchored at the frame on
//!   screen, and the sound's state says why (Swift's never-prime watchdog);
//! - **wall clock** otherwise: DXVKit Plus's transport integrates exact clock deltas and wraps the
//!   loop exactly, so loops never drift (Swift re-anchors at each wrap for the same reason);
//! - seek, step (paused, one frame, clamped at both ends; the sound paused at that frame's
//!   start), restart (frame 0, play / pause kept), scrub (pauses, shows the latest requested frame,
//!   resumes on release only if it was playing), loop on / off in lockstep with the sound;
//! - at the end of a clip that does not loop the clock stops on the last frame, and the next play
//!   starts again from frame 0 on every path — fixed on purpose (Amrit 2026-11-14: "fix"; Swift's
//!   DXV and HAP players stayed on the last frame).
//!
//! [`ClipPlayer`] puts a decoded frame behind the clock: DXVKit Plus's read-ahead cache, asked for
//! exactly the frame the clock names (never "the nearest ready"), so a scrub never builds a
//! backlog — each refresh shows the latest frame asked for. [`Samples`] and [`Cadence`] turn a run
//! into timing numbers.
#![forbid(unsafe_code)]

pub use glance_audio::{AudioClock, AudioError, AudioState, DeviceFormat, TrackFormat, ticks};
use glance_media::transport::{Clip, Inputs, PlayMode, Settings, Transport};
use glance_media::{CacheSettings, CacheStats, Decoded, MediaError, MediaInfo, MediaTime, NotPlayable, Player, Route, TICKS_PER_SECOND, Tick, cache};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long the audio position may stay unavailable after play before the clock falls back to
/// the wall clock (Swift's `audioPrimeGrace`).
pub const AUDIO_PRIME_GRACE: Duration = Duration::from_millis(250);

/// What the clock needs from the sound (the device clock in glance-audio; a scripted one in tests).
pub trait AudioMaster {
    /// Start or resume; `false` when it cannot (the clock falls back).
    fn play(&mut self) -> bool;
    fn pause(&mut self);
    /// Continue from movie time `t`.
    fn seek(&mut self, t: MediaTime);
    fn set_loops(&mut self, loops: bool);
    /// The movie time heard at host instant `at` (the moment the picture is shown); `None` until
    /// primed.
    fn position_at(&self, at: Instant) -> Option<Tick>;
    /// A clip that does not loop has played its last audio frame.
    fn ended(&self) -> bool;
    fn set_volume(&mut self, volume: f32);
    fn set_muted(&mut self, muted: bool);
    /// Why the device stopped, when it did.
    fn failed(&self) -> Option<String>;
    /// What the viewer shows while it plays.
    fn state(&self) -> AudioState;
    /// Where the sound's loop ends on the movie timeline; `None` when unknown (the picture then
    /// holds its last frame until the sound wraps, as Swift does).
    fn end(&self) -> Option<Tick> {
        None
    }
    /// The track, the device, and what the heard time is built from (for reports).
    fn describe(&self) -> Option<AudioDescription> {
        None
    }
    /// Follow the output device (glance-audio's `AudioClock::follow_device`): `Ok(true)` when the
    /// sound was rebuilt on a new device format, `Err` when no sound is left.
    fn follow_device(&mut self) -> Result<bool, String> {
        Ok(false)
    }
}

/// The sound as it plays (for reports).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioDescription {
    pub track: TrackFormat,
    pub device: DeviceFormat,
    /// cpal's playback minus callback instant, ns, and the last callback's buffer, frames.
    pub reported_delay_nanos: u64,
    pub buffer_frames: u64,
    /// Streams rebuilt on a new device format, under- / overruns on the current stream, and the
    /// frames its callback has played.
    pub reroutes: u32,
    pub xruns: u64,
    pub rendered_frames: u64,
}

impl AudioMaster for AudioClock {
    fn play(&mut self) -> bool {
        AudioClock::play(self)
    }
    fn pause(&mut self) {
        AudioClock::pause(self);
    }
    fn seek(&mut self, t: MediaTime) {
        AudioClock::seek(self, t);
    }
    fn set_loops(&mut self, loops: bool) {
        AudioClock::set_loops(self, loops);
    }
    fn position_at(&self, at: Instant) -> Option<Tick> {
        AudioClock::position_ticks_at(self, at)
    }
    fn ended(&self) -> bool {
        AudioClock::ended(self)
    }
    fn set_volume(&mut self, volume: f32) {
        AudioClock::set_volume(self, volume);
    }
    fn set_muted(&mut self, muted: bool) {
        AudioClock::set_muted(self, muted);
    }
    fn failed(&self) -> Option<String> {
        AudioClock::failed(self)
    }
    fn state(&self) -> AudioState {
        AudioClock::state(self)
    }
    fn end(&self) -> Option<Tick> {
        self.end_ticks()
    }
    fn describe(&self) -> Option<AudioDescription> {
        let (reported_delay_nanos, buffer_frames) = self.reported_latency();
        let (reroutes, xruns, rendered_frames) = self.device_counts();
        Some(AudioDescription { track: self.track(), device: self.device(), reported_delay_nanos, buffer_frames, reroutes, xruns, rendered_frames })
    }
    fn follow_device(&mut self) -> Result<bool, String> {
        AudioClock::follow_device(self).map(|r| r.is_some()).map_err(|e| e.to_string())
    }
}

/// Which clock drives the picture this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Master {
    Audio,
    Wall,
}

/// The frame clock of one clip.
pub struct Clock {
    clip: Clip,
    settings: Settings,
    transport: Transport,
    /// The wall clock's zero.
    epoch: Instant,
    paused: bool,
    /// Play was asked for; the clock re-anchors (and the sound starts) at the next tick.
    starting: bool,
    current: u32,
    ended: bool,
    /// The end of the clip's last frame on the movie timeline.
    end: Tick,
    audio: Option<Box<dyn AudioMaster>>,
    audio_state: AudioState,
    last_audio_ok: Instant,
    volume: f32,
    muted: bool,
    scrub_resume: Option<bool>,
}

impl std::fmt::Debug for Clock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Clock")
            .field("clip", &self.clip)
            .field("current", &self.current)
            .field("paused", &self.paused)
            .field("audio", &self.audio_state)
            .finish()
    }
}

impl Clock {
    /// A clock for `clip`, looping and playing (it starts at the first [`Clock::tick`]), on the
    /// sound when `audio` is `Ok`, on the wall clock with `audio`'s state otherwise.
    pub fn new(clip: Clip, audio: Result<Box<dyn AudioMaster>, AudioError>) -> Clock {
        let end = match clip.timing.as_deref().filter(|t| t.frame_count() > 0) {
            Some(t) => glance_audio::ticks(t.end_time()),
            None => clip.rate.tick_of(i64::from(clip.frames)),
        };
        let (audio, audio_state) = match audio {
            Ok(mut a) => {
                a.set_loops(true);
                let state = a.state();
                (Some(a), state)
            }
            Err(e) => (None, e.state()),
        };
        let now = Instant::now();
        Clock {
            clip,
            settings: Settings::default(),
            transport: Transport::default(),
            epoch: now,
            paused: false,
            starting: true,
            current: 0,
            ended: false,
            end,
            audio,
            audio_state,
            last_audio_ok: now,
            volume: 1.0,
            muted: false,
            scrub_resume: None,
        }
    }

    pub fn clip(&self) -> &Clip {
        &self.clip
    }
    /// The frame on screen.
    pub fn current(&self) -> u32 {
        self.current
    }
    pub fn is_paused(&self) -> bool {
        self.paused
    }
    pub fn is_looping(&self) -> bool {
        self.settings.mode == PlayMode::Loop
    }
    pub fn audio_state(&self) -> &AudioState {
        &self.audio_state
    }
    pub fn master(&self) -> Master {
        if self.audio.is_some() { Master::Audio } else { Master::Wall }
    }
    /// The sound as it plays, when it drives the clock.
    pub fn audio_description(&self) -> Option<AudioDescription> {
        self.audio.as_ref()?.describe()
    }

    /// The movie time heard at `at`, when the sound drives the clock.
    pub fn audio_position(&self, at: Instant) -> Option<Tick> {
        self.audio.as_ref()?.position_at(at)
    }

    fn last(&self) -> u32 {
        self.clip.frames.max(1).saturating_sub(1)
    }

    /// Frame `n`'s real start on the movie timeline.
    pub fn time_of_frame(&self, n: u32) -> MediaTime {
        match self.clip.timing.as_deref().filter(|t| t.frame_count() > 0) {
            Some(t) => t.time_of_frame(n),
            None => MediaTime::new(i64::from(n).saturating_mul(self.clip.rate.den), u32::try_from(self.clip.rate.num).unwrap_or(1)),
        }
    }

    fn wall(&self, now: Instant) -> Tick {
        Tick::from_units(i64::try_from(now.saturating_duration_since(self.epoch).as_nanos()).unwrap_or(i64::MAX), 1_000_000_000)
    }

    /// Back to the wall clock, re-anchored at the frame on screen; the sound's state says why.
    fn fall_back(&mut self, reason: String) {
        if let Some(mut a) = self.audio.take() {
            a.pause();
        }
        self.audio_state = AudioState::Failed { reason };
        self.transport.seek(self.current, &self.clip, &self.settings);
    }

    /// The frame for clock time `now` — pass the moment the picture will be on screen: on the
    /// audio clock the frame is the one nearest the sound heard at that moment.
    pub fn tick(&mut self, now: Instant) -> u32 {
        if self.paused {
            return self.current;
        }
        // A new output device format: the sound is rebuilt at the position heard and primes again,
        // so the quarter-second grace starts over.
        match self.audio.as_mut().map(|a| a.follow_device()) {
            Some(Ok(true)) => self.last_audio_ok = now,
            Some(Err(reason)) => self.fall_back(format!("audio device lost: {reason}")),
            Some(Ok(false)) | None => {}
        }
        if self.starting {
            self.starting = false;
            self.transport.seek(self.current, &self.clip, &self.settings);
            self.last_audio_ok = now;
            if let Some(a) = self.audio.as_mut()
                && !a.play()
            {
                self.fall_back("audio engine failed to start".into());
            }
        }
        if let Some(reason) = self.audio.as_ref().and_then(|a| a.failed()) {
            self.fall_back(format!("audio device failed: {reason}"));
        }
        if let Some(a) = self.audio.as_ref() {
            let Some(t) = a.position_at(now) else {
                if now.saturating_duration_since(self.last_audio_ok) > AUDIO_PRIME_GRACE {
                    self.fall_back("audio failed to start".into());
                    return self.tick_wall(now);
                }
                return self.current;
            };
            self.last_audio_ok = now;
            self.current = self.clip.nearest_frame(t);
            if self.is_looping() && self.current == self.last() && self.seam_wraps(t, a.end()) {
                self.current = 0;
            }
            if !self.is_looping() && (t >= self.end || a.ended()) {
                self.current = self.last();
                self.stop_at_end();
            }
            return self.current;
        }
        self.tick_wall(now)
    }

    /// On the audio clock, the half frame before the loop seam: the frame nearest the sound is the
    /// next loop's frame 0 (when the sound's loop is no longer than the picture's). Swift clamps to
    /// the last frame there, so the last frame showed for one and a half frames and frame 0 for half
    /// a frame — measured: frame 0 skipped once in 20 loops when a present came late (C2). Fixed on
    /// purpose. A sound longer than the picture still holds the last frame until it wraps (Swift).
    fn seam_wraps(&self, t: Tick, audio_end: Option<Tick>) -> bool {
        let last_start = match self.clip.timing.as_deref().filter(|ft| ft.frame_count() > 0) {
            Some(ft) => glance_audio::ticks(ft.time_of_frame(self.last())),
            None => self.clip.rate.tick_of(i64::from(self.last())),
        };
        let half = Tick(self.end.0.saturating_sub(last_start.0) / 2);
        let middle = Tick(last_start.0.saturating_add(half.0));
        t >= middle && audio_end.is_some_and(|e| e.0 <= self.end.0.saturating_add(half.0))
    }

    fn tick_wall(&mut self, now: Instant) -> u32 {
        let at = self.transport.advance(self.wall(now), &self.clip, &self.settings, &Inputs::default());
        self.current = at.frame.min(self.last());
        if at.ended {
            self.stop_at_end();
        }
        self.current
    }

    fn stop_at_end(&mut self) {
        self.ended = true;
        self.paused = true;
        if let Some(a) = self.audio.as_mut() {
            a.pause();
        }
    }

    /// Play. At the end of a clip that does not loop (or on its last frame), play starts again
    /// from frame 0.
    pub fn play(&mut self) {
        if !self.paused {
            return;
        }
        if self.ended || (!self.is_looping() && self.current >= self.last()) {
            self.seek(0);
        }
        self.paused = false;
        self.starting = true;
    }

    pub fn pause(&mut self) {
        self.paused = true;
        self.starting = false;
        if let Some(a) = self.audio.as_mut() {
            a.pause();
        }
    }

    pub fn toggle(&mut self) {
        if self.paused { self.play() } else { self.pause() }
    }

    /// Show frame `n` (clamped), the sound moved to its start; play / pause kept.
    pub fn seek(&mut self, n: u32) {
        let n = n.min(self.last());
        self.current = n;
        self.ended = false;
        self.transport.seek(n, &self.clip, &self.settings);
        let t = self.time_of_frame(n);
        if let Some(a) = self.audio.as_mut() {
            a.seek(t);
        }
        if !self.paused {
            // Re-anchor at the next tick (the sound restarts from the new frame).
            self.starting = true;
        }
    }

    /// One frame back or forward, paused, clamped at both ends; the sound paused at that frame.
    pub fn step(&mut self, forward: bool) {
        self.pause();
        let n = if forward { self.current.saturating_add(1) } else { self.current.saturating_sub(1) };
        self.seek(n);
    }

    /// Frame 0, play / pause kept.
    pub fn restart(&mut self) {
        self.seek(0);
    }

    /// A scrub drag starts: pause, remembering whether it was playing.
    pub fn begin_scrub(&mut self) {
        if self.scrub_resume.is_none() {
            self.scrub_resume = Some(!self.paused);
        }
        self.pause();
    }

    /// The drag moved to frame `n`.
    pub fn scrub_to(&mut self, n: u32) {
        self.seek(n);
    }

    /// The drag ended: resume only if it was playing.
    pub fn end_scrub(&mut self) {
        if self.scrub_resume.take() == Some(true) {
            self.play();
        }
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.settings.mode = if looping { PlayMode::Loop } else { PlayMode::OnceHold };
        if let Some(a) = self.audio.as_mut() {
            a.set_loops(looping);
        }
        if looping {
            self.ended = false;
        }
    }

    /// Output level 0…1; never moves the clock.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        if let Some(a) = self.audio.as_mut() {
            a.set_volume(volume);
        }
    }

    /// Mute (separate from the volume); never moves the clock.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        if let Some(a) = self.audio.as_mut() {
            a.set_muted(muted);
        }
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }
    pub fn muted(&self) -> bool {
        self.muted
    }
}

/// The frame that belongs on screen now.
#[derive(Clone, Debug)]
pub struct Shown {
    /// The file frame index.
    pub index: u32,
    pub frame: Arc<Decoded>,
    /// How long the caller waited for it (about zero on a read-ahead hit; a decode on a miss).
    pub fetch: Duration,
}

/// Whether a clip's sound is played.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// The first audio track on the default output device (Swift Glance).
    Device,
    /// No sound: the wall clock (measurements without audio).
    Off,
}

/// One clip playing: the clock and the decoded frame it names.
pub struct ClipPlayer {
    player: Player,
    route: Route,
    clock: Clock,
    read_ahead: usize,
}

impl std::fmt::Debug for ClipPlayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClipPlayer").field("route", &self.route).field("clock", &self.clock).finish()
    }
}

/// The transport's clip for a source: its frame timing (real frame starts on a table).
pub fn clip_of(source: &dyn glance_media::FrameSource) -> Clip {
    let timing = source.timing();
    if timing.frame_count() > 0 { Clip::from_timing(Arc::new(timing)) } else { Clip::new(1, source.info().rate) }
}

impl ClipPlayer {
    /// Route and open `path` with its sound, looping and playing; a file nothing plays comes back
    /// as the line the viewer shows. Its clock starts with the first [`ClipPlayer::frame_at`], so
    /// time spent opening windows never eats frames.
    pub fn open(path: &Path, cache: CacheSettings) -> Result<ClipPlayer, NotPlayable> {
        Self::open_with(path, cache, Sound::Device)
    }

    pub fn open_with(path: &Path, cache: CacheSettings, sound: Sound) -> Result<ClipPlayer, NotPlayable> {
        let routed = glance_media::route(path)?;
        let source = routed.open()?;
        let clip = clip_of(source.as_ref());
        let audio: Result<Box<dyn AudioMaster>, AudioError> = match sound {
            Sound::Device => AudioClock::open(path, &routed.info.audio, true).map(|a| Box::new(a) as Box<dyn AudioMaster>),
            Sound::Off => Err(AudioError::Off),
        };
        let cache = cache.clamped();
        Ok(ClipPlayer { player: Player::new(source, cache), route: routed.route, clock: Clock::new(clip, audio), read_ahead: cache.read_ahead })
    }

    pub fn info(&self) -> &MediaInfo {
        self.player.info()
    }

    /// Which engine plays the clip.
    pub fn route(&self) -> Route {
        self.route
    }

    pub fn cache_stats(&self) -> CacheStats {
        self.player.stats()
    }

    pub fn clock(&self) -> &Clock {
        &self.clock
    }

    /// The transport (play, pause, seek, step, restart, scrub, loop, volume, mute).
    pub fn clock_mut(&mut self) -> &mut Clock {
        &mut self.clock
    }

    /// The frame for clock time `now` (the moment it will be shown), decoded — exactly that frame,
    /// never "the nearest ready".
    pub fn frame_at(&mut self, now: Instant) -> Result<Shown, MediaError> {
        let index = self.clock.tick(now);
        let ahead = if self.clock.is_paused() { Vec::new() } else { cache::ahead(index, 1, 1, 0, self.clock.clip().frames, self.read_ahead) };
        let asked = Instant::now();
        let frame = self.player.get(index, &ahead)?;
        Ok(Shown { index, frame, fetch: asked.elapsed() })
    }
}

/// A media time in seconds (for reports).
pub fn seconds(t: Tick) -> f64 {
    t.0 as f64 / TICKS_PER_SECOND as f64
}

/// Timing samples in milliseconds.
#[derive(Clone, Debug, Default)]
pub struct Samples(Vec<f64>);

/// What a set of samples looks like.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct Summary {
    pub count: usize,
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

impl Samples {
    pub fn push(&mut self, d: Duration) {
        self.0.push(d.as_secs_f64() * 1000.0);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Nearest-rank percentiles; `None` without samples.
    pub fn summary(&self) -> Option<Summary> {
        let mut v = self.0.clone();
        v.sort_by(f64::total_cmp);
        let max = *v.last()?;
        let rank = |p: f64| {
            let i = ((p * v.len() as f64).ceil() as usize).clamp(1, v.len());
            v.get(i.saturating_sub(1)).copied().unwrap_or(max)
        };
        Some(Summary {
            count: v.len(),
            mean_ms: v.iter().sum::<f64>() / v.len() as f64,
            p50_ms: rank(0.50),
            p95_ms: rank(0.95),
            p99_ms: rank(0.99),
            max_ms: max,
        })
    }
}

/// Which file frames reached the screen, in loop order: how many distinct showings, and how many
/// frames were jumped over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Cadence {
    /// Times the frame on screen changed (plus the first one).
    pub shown: u64,
    /// File frames jumped over between two consecutive showings (0 = every frame was shown).
    pub skipped: u64,
    #[serde(skip)]
    last: Option<u32>,
}

impl Cadence {
    /// Record the frame index on screen this refresh; `true` when it is a new frame.
    pub fn show(&mut self, index: u32, frames: u32) -> bool {
        let Some(last) = self.last else {
            self.last = Some(index);
            self.shown = 1;
            return true;
        };
        if index == last {
            return false;
        }
        let n = i64::from(frames.max(1));
        let step = (i64::from(index) - i64::from(last)).rem_euclid(n);
        self.skipped = self.skipped.saturating_add(u64::try_from(step.saturating_sub(1)).unwrap_or(0));
        self.shown = self.shown.saturating_add(1);
        self.last = Some(index);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glance_media::FrameRate;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// What the scripted sound was told, and what it answers.
    #[derive(Debug, Default)]
    struct Script {
        position: Option<Tick>,
        playing: bool,
        loops: bool,
        ended: bool,
        refuse_play: bool,
        failed: Option<String>,
        end: Option<Tick>,
        seeks: Vec<MediaTime>,
        gain: (f32, bool),
        /// What the next `follow_device` answers (then back to "nothing changed").
        follow: Option<Result<bool, String>>,
    }

    struct Fake(Rc<RefCell<Script>>);

    impl AudioMaster for Fake {
        fn play(&mut self) -> bool {
            let mut s = self.0.borrow_mut();
            s.playing = !s.refuse_play;
            !s.refuse_play
        }
        fn pause(&mut self) {
            self.0.borrow_mut().playing = false;
        }
        fn seek(&mut self, t: MediaTime) {
            self.0.borrow_mut().seeks.push(t);
        }
        fn set_loops(&mut self, loops: bool) {
            self.0.borrow_mut().loops = loops;
        }
        fn position_at(&self, _at: Instant) -> Option<Tick> {
            self.0.borrow().position
        }
        fn ended(&self) -> bool {
            self.0.borrow().ended
        }
        fn set_volume(&mut self, v: f32) {
            self.0.borrow_mut().gain.0 = v;
        }
        fn set_muted(&mut self, m: bool) {
            self.0.borrow_mut().gain.1 = m;
        }
        fn failed(&self) -> Option<String> {
            self.0.borrow().failed.clone()
        }
        fn state(&self) -> AudioState {
            AudioState::Playing { sample_rate: 48_000, channels: 2 }
        }
        fn end(&self) -> Option<Tick> {
            self.0.borrow().end
        }
        fn follow_device(&mut self) -> Result<bool, String> {
            self.0.borrow_mut().follow.take().unwrap_or(Ok(false))
        }
    }

    const FPS: FrameRate = FrameRate::FPS_30;

    fn with_audio(frames: u32) -> (Clock, Rc<RefCell<Script>>, Instant) {
        let script = Rc::new(RefCell::new(Script::default()));
        let clock = Clock::new(Clip::new(frames, FPS), Ok(Box::new(Fake(script.clone()))));
        (clock, script, Instant::now())
    }

    fn wall(frames: u32) -> (Clock, Instant) {
        (Clock::new(Clip::new(frames, FPS), Err(AudioError::NoAudioTrack)), Instant::now())
    }

    /// `s` seconds after the start, plus 1 us: a time exactly on a frame start converts to whole
    /// nanoseconds a hair early and would floor to the frame before.
    fn secs(s: f64) -> Duration {
        Duration::from_secs_f64(s) + Duration::from_micros(1)
    }

    fn at(s: f64) -> Tick {
        Tick((s * TICKS_PER_SECOND as f64).round() as i64)
    }

    #[test]
    fn the_audio_master_shows_the_nearest_frame_clamped() {
        let (mut c, a, t0) = with_audio(10);
        assert_eq!(c.master(), Master::Audio);
        a.borrow_mut().position = Some(at(0.0));
        assert_eq!(c.tick(t0), 0);
        assert!(a.borrow().playing && a.borrow().loops, "play starts the sound, looping");
        // Half a frame (1/60 s) is a tie: the later frame (Swift: round half away from zero).
        a.borrow_mut().position = Some(Tick(TICKS_PER_SECOND / 60 - 1));
        assert_eq!(c.tick(t0 + secs(0.1)), 0);
        a.borrow_mut().position = Some(Tick(TICKS_PER_SECOND / 60));
        assert_eq!(c.tick(t0 + secs(0.1)), 1);
        // The wall clock is not consulted: the frame is the sound's even when time stands still.
        a.borrow_mut().position = Some(at(5.0 / 30.0));
        assert_eq!(c.tick(t0 + secs(0.1)), 5);
        // A longer sound: the picture holds its last frame while looping.
        a.borrow_mut().position = Some(at(2.0));
        assert_eq!(c.tick(t0 + secs(0.2)), 9);
        assert!(!c.is_paused());
    }

    #[test]
    fn without_audio_progress_for_a_quarter_second_the_wall_clock_takes_over() {
        let (mut c, a, t0) = with_audio(300);
        a.borrow_mut().position = Some(at(3.0 / 30.0));
        assert_eq!(c.tick(t0), 3);
        // The sound stops answering: the frame is held up to the grace...
        a.borrow_mut().position = None;
        assert_eq!(c.tick(t0 + secs(0.2)), 3);
        assert_eq!(c.tick(t0 + secs(0.249)), 3);
        assert_eq!(c.master(), Master::Audio);
        // ...then the wall clock, re-anchored at the frame on screen (no jump back or forward).
        assert_eq!(c.tick(t0 + secs(0.26)), 3);
        assert_eq!(c.master(), Master::Wall);
        assert_eq!(c.audio_state(), &AudioState::Failed { reason: "audio failed to start".into() });
        assert!(!a.borrow().playing, "the dead sound is stopped");
        // (Measured from the re-anchor tick, so a little past it again.)
        assert_eq!(c.tick(t0 + secs(0.26 + 10.0 / 30.0) + Duration::from_micros(5)), 13);
    }

    #[test]
    fn a_sound_rebuilt_on_a_new_device_gets_the_quarter_second_again_and_a_lost_one_falls_back() {
        let (mut c, a, t0) = with_audio(300);
        a.borrow_mut().position = Some(at(3.0 / 30.0));
        assert_eq!(c.tick(t0), 3);
        // The device's rate changes 0.2 s later: the sound is rebuilt and primes again. The grace
        // runs from the rebuild, not from the last position heard.
        a.borrow_mut().position = None;
        assert_eq!(c.tick(t0 + secs(0.1)), 3);
        a.borrow_mut().follow = Some(Ok(true));
        assert_eq!(c.tick(t0 + secs(0.2)), 3);
        assert_eq!(c.tick(t0 + secs(0.4)), 3);
        assert_eq!(c.master(), Master::Audio, "0.2 s after the rebuild: still the sound's clock");
        a.borrow_mut().position = Some(at(4.0 / 30.0));
        assert_eq!(c.tick(t0 + secs(0.42)), 4);
        // No sound left after a device change: the wall clock at once, from the frame on screen.
        a.borrow_mut().follow = Some(Err("no audio output device".into()));
        assert_eq!(c.tick(t0 + secs(0.43)), 4);
        assert_eq!(c.master(), Master::Wall);
        assert_eq!(c.audio_state(), &AudioState::Failed { reason: "audio device lost: no audio output device".into() });
    }

    #[test]
    fn a_sound_that_cannot_start_or_fails_falls_back_at_once() {
        let (mut c, a, t0) = with_audio(30);
        a.borrow_mut().refuse_play = true;
        assert_eq!(c.tick(t0), 0);
        assert_eq!(c.audio_state(), &AudioState::Failed { reason: "audio engine failed to start".into() });
        assert_eq!(c.tick(t0 + secs(2.0 / 30.0) + Duration::from_micros(5)), 2);

        let (mut c, a, t0) = with_audio(30);
        a.borrow_mut().position = Some(at(0.0));
        c.tick(t0);
        a.borrow_mut().failed = Some("device unplugged".into());
        c.tick(t0 + secs(0.05));
        assert_eq!(c.master(), Master::Wall);
        assert_eq!(c.audio_state(), &AudioState::Failed { reason: "audio device failed: device unplugged".into() });
        // Negative control: a clip without audio is on the wall clock from the start, state None.
        let (c, _) = wall(30);
        assert_eq!((c.master(), c.audio_state().clone()), (Master::Wall, AudioState::None { reason: "no audio track".into() }));
    }

    #[test]
    fn the_wall_clock_does_not_drift_across_twenty_loops() {
        let (mut c, t0) = wall(90);
        assert_eq!(c.tick(t0), 0);
        // Ticks at 120 Hz for 20 loops of 3 s; every tick shows floor(t * 30) mod 90.
        let mut wrong = 0;
        for i in 1..=(20 * 3 * 120) {
            let t = f64::from(i) / 120.0;
            let want = ((i * 30 / 120) % 90) as u32;
            if c.tick(t0 + secs(t)) != want {
                wrong += 1;
            }
        }
        assert_eq!(wrong, 0);
        assert_eq!(c.tick(t0 + secs(60.0 + 7.0 / 30.0)), 7);
    }

    #[test]
    fn seek_step_restart_and_scrub_follow_swift_glance() {
        let (mut c, a, t0) = with_audio(100);
        a.borrow_mut().position = Some(at(0.0));
        c.tick(t0);
        c.seek(40);
        assert_eq!(c.current(), 40);
        assert_eq!(a.borrow().seeks.last().copied(), Some(MediaTime::new(40, 30)));
        assert!(!c.is_paused());
        // Step: pauses (the sound too), one frame, its start sent to the sound; clamped at both ends.
        c.step(true);
        assert!(c.is_paused() && !a.borrow().playing);
        assert_eq!((c.current(), a.borrow().seeks.last().copied()), (41, Some(MediaTime::new(41, 30))));
        assert_eq!(c.tick(t0 + secs(1.0)), 41, "paused: time does not move it");
        c.seek(0);
        c.step(false);
        assert_eq!(c.current(), 0);
        c.seek(99);
        c.step(true);
        assert_eq!(c.current(), 99);
        // Restart keeps pause.
        c.restart();
        assert_eq!((c.current(), c.is_paused()), (0, true));
        // Scrub from playing: pauses, each request shown, resumes on release.
        c.play();
        a.borrow_mut().position = Some(at(0.0));
        c.tick(t0 + secs(1.0));
        c.begin_scrub();
        assert!(c.is_paused());
        for n in [10, 55, 30] {
            c.scrub_to(n);
            assert_eq!(c.tick(t0 + secs(1.1)), n);
        }
        c.end_scrub();
        assert!(!c.is_paused());
        // Scrub from paused: stays paused.
        c.pause();
        c.begin_scrub();
        c.scrub_to(12);
        c.end_scrub();
        assert!(c.is_paused());
        assert_eq!(c.current(), 12);
    }

    #[test]
    fn at_the_end_of_a_clip_that_does_not_loop_play_starts_again_from_frame_0() {
        // Wall clock.
        let (mut c, t0) = wall(30);
        c.set_looping(false);
        c.tick(t0);
        assert_eq!(c.tick(t0 + secs(0.5)), 15);
        assert_eq!(c.tick(t0 + secs(1.5)), 29);
        assert!(c.is_paused(), "stops on the last frame");
        assert_eq!(c.tick(t0 + secs(3.0)), 29);
        c.play();
        assert_eq!(c.current(), 0);
        assert_eq!(c.tick(t0 + secs(3.0)), 0);
        assert_eq!(c.tick(t0 + secs(3.0 + 4.0 / 30.0) + Duration::from_micros(5)), 4);
        // Audio master: the sound past the picture's end stops it; play seeks the sound to 0.
        let (mut c, a, t0) = with_audio(30);
        c.set_looping(false);
        assert!(!a.borrow().loops);
        a.borrow_mut().position = Some(at(0.5));
        assert_eq!(c.tick(t0), 15);
        a.borrow_mut().position = Some(at(1.0));
        assert_eq!(c.tick(t0 + secs(0.5)), 29);
        assert!(c.is_paused() && !a.borrow().playing);
        c.play();
        assert_eq!(c.current(), 0);
        assert_eq!(a.borrow().seeks.last().copied(), Some(MediaTime::new(0, 30)));
        // A shorter sound that ended stops the picture too (Swift kept "playing" on a held frame).
        let (mut c, a, t0) = with_audio(30);
        c.set_looping(false);
        a.borrow_mut().position = Some(at(0.4));
        c.tick(t0);
        a.borrow_mut().ended = true;
        assert_eq!(c.tick(t0 + secs(0.1)), 29);
        assert!(c.is_paused());
    }

    #[test]
    fn at_the_loop_seam_the_nearest_frame_is_the_next_loops_frame_0() {
        // 10 frames at 30 fps; the sound loops at the same length (1/3 s).
        let (mut c, a, t0) = with_audio(10);
        a.borrow_mut().end = Some(at(10.0 / 30.0));
        a.borrow_mut().position = Some(at(9.0 / 30.0));
        assert_eq!(c.tick(t0), 9);
        // Before the last frame's middle: still frame 9; from its middle on: frame 0.
        a.borrow_mut().position = Some(Tick(at(9.5 / 30.0).0 - 1));
        assert_eq!(c.tick(t0 + secs(0.01)), 9);
        a.borrow_mut().position = Some(at(9.5 / 30.0));
        assert_eq!(c.tick(t0 + secs(0.02)), 0);
        // A sound longer than the picture: the last frame is held (Swift).
        a.borrow_mut().end = Some(at(2.0));
        assert_eq!(c.tick(t0 + secs(0.03)), 9);
        // Not looping: no wrap, the end rule applies.
        a.borrow_mut().end = Some(at(10.0 / 30.0));
        c.set_looping(false);
        a.borrow_mut().position = Some(at(9.6 / 30.0));
        assert_eq!(c.tick(t0 + secs(0.04)), 9);
    }

    #[test]
    fn a_one_frame_clip_shows_its_frame() {
        let (mut c, t0) = wall(1);
        for s in [0.0, 0.01, 0.5, 3.0] {
            assert_eq!(c.tick(t0 + secs(s)), 0);
        }
        c.set_looping(false);
        c.tick(t0 + secs(4.0));
        c.play();
        assert_eq!(c.tick(t0 + secs(5.0)), 0);
        let (mut c, a, t0) = with_audio(1);
        a.borrow_mut().position = Some(at(0.7));
        assert_eq!(c.tick(t0), 0);
    }

    #[test]
    fn volume_and_mute_never_move_the_clock() {
        let (mut c, a, t0) = with_audio(100);
        a.borrow_mut().position = Some(at(20.0 / 30.0));
        assert_eq!(c.tick(t0), 20);
        let seeks = a.borrow().seeks.len();
        c.set_volume(0.0);
        c.set_muted(true);
        assert_eq!(c.tick(t0 + secs(0.01)), 20);
        c.set_muted(false);
        c.set_volume(0.7);
        assert_eq!(a.borrow().gain, (0.7, false));
        assert_eq!(a.borrow().seeks.len(), seeks, "no reposition");
        assert!(a.borrow().playing && !c.is_paused());
        assert_eq!(c.master(), Master::Audio);
    }

    #[test]
    fn percentiles_are_nearest_rank() {
        let mut s = Samples::default();
        assert!(s.summary().is_none());
        for ms in 1..=100u64 {
            s.push(Duration::from_millis(ms));
        }
        let sum = s.summary().unwrap();
        assert_eq!((sum.count, sum.p50_ms, sum.p95_ms, sum.p99_ms, sum.max_ms), (100, 50.0, 95.0, 99.0, 100.0));
        assert!((sum.mean_ms - 50.5).abs() < 1e-9);
    }

    #[test]
    fn cadence_counts_showings_and_skips_across_the_loop_point() {
        let mut c = Cadence::default();
        // 0 0 1 2 (repeat = no new showing), then 2 -> 5 skips 3 and 4, then 5 -> 0 of a 6-frame loop skips none.
        let news: Vec<bool> = [0, 0, 1, 2, 5, 0].iter().map(|&i| c.show(i, 6)).collect();
        assert_eq!(news, [true, false, true, true, true, true]);
        assert_eq!((c.shown, c.skipped), (5, 2));
        // Negative control: a jump over the loop point is counted too (4 -> 1 of 6 skips 5 and 0).
        let mut c = Cadence::default();
        c.show(4, 6);
        c.show(1, 6);
        assert_eq!(c.skipped, 2);
    }
}
