//! Audio for an encode (Swift GlEnc 1.6.2 `AudioSupport.swift`, `EncodeQueue.swift:913-956`): the first source
//! track decoded to interleaved 32-bit signed little-endian PCM at the chosen rate, capped at the container's limit
//! (AAC in `.mp4`: 48 kHz). An audio problem never fails the job: the video ships and the reason is kept as a warning.

use crate::codec::OutputContainer;

/// The rate menu (Swift's `AudioRate`; persisted as 0 / 44100 / 48000 / 88200 / 96000).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AudioRate {
    #[default]
    Original,
    Hz44100,
    Hz48000,
    Hz88200,
    Hz96000,
}

impl AudioRate {
    pub const ALL: [AudioRate; 5] = [AudioRate::Original, AudioRate::Hz44100, AudioRate::Hz48000, AudioRate::Hz88200, AudioRate::Hz96000];
    pub fn hz(self) -> Option<u32> {
        match self {
            AudioRate::Original => None,
            AudioRate::Hz44100 => Some(44_100),
            AudioRate::Hz48000 => Some(48_000),
            AudioRate::Hz88200 => Some(88_200),
            AudioRate::Hz96000 => Some(96_000),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            AudioRate::Original => "Original",
            AudioRate::Hz44100 => "44.1 kHz",
            AudioRate::Hz48000 => "48 kHz",
            AudioRate::Hz88200 => "88.2 kHz",
            AudioRate::Hz96000 => "96 kHz",
        }
    }
    pub fn persist_int(self) -> i64 {
        self.hz().map_or(0, i64::from)
    }
    pub fn from_persist_int(v: i64) -> AudioRate {
        match v {
            44_100 => AudioRate::Hz44100,
            48_000 => AudioRate::Hz48000,
            88_200 => AudioRate::Hz88200,
            96_000 => AudioRate::Hz96000,
            _ => AudioRate::Original,
        }
    }
}

/// Audio on / off and the rate (Swift's `AudioPlan`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AudioPlan {
    pub enabled: bool,
    pub rate: AudioRate,
}

impl Default for AudioPlan {
    fn default() -> AudioPlan {
        AudioPlan { enabled: true, rate: AudioRate::Original }
    }
}

/// Decoded audio: interleaved s32le.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pcm {
    pub rate: u32,
    pub channels: u32,
    pub data: Vec<u8>,
}

impl Pcm {
    pub fn bytes_per_frame(&self) -> usize {
        self.channels as usize * 4
    }
    pub fn frames(&self) -> u64 {
        let b = self.bytes_per_frame();
        self.data.len().checked_div(b).unwrap_or(0) as u64
    }
    /// The PCM of `[start, end)` seconds (trimmed jobs carry the audio of their picture: Amrit 2026-11-14, "fix").
    pub fn slice_seconds(&self, start: f64, end: f64) -> Pcm {
        let b = self.bytes_per_frame();
        let n = self.frames();
        let at = |s: f64| ((s * f64::from(self.rate)).round().max(0.0) as u64).min(n) as usize * b;
        let (a, z) = (at(start), at(end.max(start)));
        Pcm { rate: self.rate, channels: self.channels, data: self.data.get(a..z).map(<[u8]>::to_vec).unwrap_or_default() }
    }
}

/// The audio a job writes, or the warning that replaces it (`EncodeQueue.swift:913-956`): the requested rate
/// clamped to the container's cap; a source above the cap re-read at the cap.
pub fn prepare(path: &std::path::Path, plan: AudioPlan, container: OutputContainer) -> (Option<Pcm>, Option<String>) {
    if !plan.enabled {
        return (None, None);
    }
    let cap = container.max_audio_rate();
    let first = plan.rate.hz().map(|h| h.min(cap));
    match crate::platform::read_pcm(path, first) {
        Ok(Some(p)) if p.rate > cap => match crate::platform::read_pcm(path, Some(cap)) {
            Ok(Some(r)) if !r.data.is_empty() => (Some(r), None),
            Ok(_) => (None, None),
            Err(_) => (None, Some(format!("Audio unavailable: couldn't resample {} Hz down to the container's {cap} Hz limit", p.rate))),
        },
        Ok(Some(p)) if !p.data.is_empty() => (Some(p), None),
        Ok(_) => (None, None),
        Err(e) => (None, Some(format!("Audio unavailable: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_persistence_is_swifts() {
        for r in AudioRate::ALL {
            assert_eq!(AudioRate::from_persist_int(r.persist_int()), r);
        }
        assert_eq!(AudioRate::from_persist_int(12345), AudioRate::Original);
    }

    #[test]
    fn slice_takes_whole_frames() {
        let p = Pcm { rate: 10, channels: 2, data: (0..80u8).collect() };
        let s = p.slice_seconds(0.2, 0.5);
        assert_eq!(s.data.len(), 3 * 8);
        assert_eq!(s.data[0], 16);
    }
}
