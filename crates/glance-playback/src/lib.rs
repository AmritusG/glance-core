//! # glance-playback — a clip on a clock, and the numbers that say how it played
//!
//! [`ClipPlayer`] joins DXVKit Plus's read-ahead cache (`cache::Player`) and exact-time transport
//! (`transport::Transport`) to a host clock: ask it which frame belongs on screen at an `Instant`
//! and it answers with that exact frame, decoded. [`Samples`] and [`Cadence`] turn a run into
//! timing numbers (percentiles, frames shown, frames skipped).
#![forbid(unsafe_code)]

use glance_media::transport::{Clip, Inputs, Settings, Transport};
use glance_media::{CacheSettings, CacheStats, Decoded, MediaError, MediaInfo, NotPlayable, Player, Route, Tick, cache};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The frame that belongs on screen now.
#[derive(Clone, Debug)]
pub struct Shown {
    /// The file frame index.
    pub index: u32,
    pub frame: Arc<Decoded>,
    /// How long the caller waited for it (about zero on a read-ahead hit; a decode on a miss).
    pub fetch: Duration,
}

/// One clip playing against the host clock.
#[derive(Debug)]
pub struct ClipPlayer {
    player: Player,
    route: Route,
    transport: Transport,
    clip: Clip,
    settings: Settings,
    read_ahead: usize,
    /// The clock time of media time zero: the first [`ClipPlayer::frame_at`].
    epoch: Option<Instant>,
}

impl ClipPlayer {
    /// Route and open `path` (looping, forward, speed 1); a file nothing plays comes back as the
    /// line the viewer shows. Its clock starts with the first [`ClipPlayer::frame_at`], so time
    /// spent opening windows never eats frames.
    pub fn open(path: &Path, cache: CacheSettings) -> Result<ClipPlayer, NotPlayable> {
        let routed = glance_media::route(path)?;
        let source = routed.open()?;
        let cache = cache.clamped();
        let timing = source.timing();
        let clip = if timing.frame_count() > 0 { Clip::from_timing(Arc::new(timing)) } else { Clip::new(1, routed.info.rate) };
        Ok(ClipPlayer {
            player: Player::new(source, cache),
            route: routed.route,
            transport: Transport::default(),
            clip,
            settings: Settings::default(),
            read_ahead: cache.read_ahead,
            epoch: None,
        })
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

    /// The frame for clock time `now`, decoded (exactly that frame, never "the nearest ready").
    pub fn frame_at(&mut self, now: Instant) -> Result<Shown, MediaError> {
        let epoch = *self.epoch.get_or_insert(now);
        let nanos = i64::try_from(now.saturating_duration_since(epoch).as_nanos()).unwrap_or(i64::MAX);
        let at = self.transport.advance(Tick::from_units(nanos, 1_000_000_000), &self.clip, &self.settings, &Inputs::default());
        let ahead = cache::ahead(at.frame, at.heading, 1, 0, self.clip.frames, self.read_ahead);
        let asked = Instant::now();
        let frame = self.player.get(at.frame, &ahead)?;
        Ok(Shown { index: at.frame, frame, fetch: asked.elapsed() })
    }
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
