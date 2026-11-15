//! `glance-proof scrub`: a scrubber dragged to a new random frame every 8.33 ms (a 120 Hz drag)
//! without a window. Each request is answered with exactly the frame asked for, decoded then —
//! no queue, so no backlog: the time from request to frame is one decode (or a cache hit). Right
//! after each request the same frame is decoded once more from a second source, with the same
//! spacing: "one decode time".

use glance_media::CacheSettings;
use glance_playback::{ClipPlayer, Samples, Sound, Summary};
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(serde::Serialize)]
struct Report {
    build: &'static str,
    clip: String,
    codec: String,
    width: u32,
    height: u32,
    frames: u32,
    requests: usize,
    /// Requests answered with another frame than the latest asked for.
    not_latest: usize,
    /// Request to decoded frame, ms.
    request_to_frame: Option<Summary>,
    /// The same frame decoded from a second source right after each request (same spacing), ms.
    decode: Option<Summary>,
    cache_hits: u64,
    cache_misses: u64,
}

pub fn run(rest: &[String]) -> Result<(), String> {
    let (mut seconds, mut out, mut clip) = (10.0f64, None::<PathBuf>, None::<PathBuf>);
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--seconds" => seconds = it.next().ok_or("--seconds needs a value")?.parse().map_err(|e| format!("--seconds: {e}"))?,
            "--out" => out = Some(it.next().ok_or("--out needs a value")?.into()),
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            path => clip = Some(path.into()),
        }
    }
    let clip = clip.ok_or("usage: glance-proof scrub [--seconds N] [--out report.json] <clip>")?;
    let mut player = ClipPlayer::open_with(&clip, CacheSettings { capacity: 24, read_ahead: 8, threads: 4 }, Sound::Off).map_err(|e| e.to_string())?;
    let mut cold = glance_media::route(&clip).and_then(|c| c.open()).map_err(|e| e.to_string())?;
    let frames = player.clock().clip().frames;
    player.clock_mut().begin_scrub();
    let (mut latency, mut decode) = (Samples::default(), Samples::default());
    let (mut requests, mut not_latest) = (0usize, 0usize);
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let step = Duration::from_secs_f64(1.0 / 120.0);
    let start = Instant::now();
    let mut next = start;
    while start.elapsed().as_secs_f64() < seconds {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let n = u32::try_from(x % u64::from(frames.max(1))).unwrap_or(0);
        let t = Instant::now();
        player.clock_mut().scrub_to(n);
        let shown = player.frame_at(t).map_err(|e| e.to_string())?;
        latency.push(t.elapsed());
        let c = Instant::now();
        cold.decode(n).map_err(|e| e.to_string())?;
        decode.push(c.elapsed());
        requests += 1;
        not_latest += usize::from(shown.index != n);
        next += step;
        if let Some(wait) = next.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
    player.clock_mut().end_scrub();
    let info = player.info();
    let cache = player.cache_stats();
    let report = Report {
        build: if cfg!(debug_assertions) { "debug" } else { "release" },
        clip: clip.display().to_string(),
        codec: info.codec_name.clone(),
        width: info.width,
        height: info.height,
        frames,
        requests,
        not_latest,
        request_to_frame: latency.summary(),
        decode: decode.summary(),
        cache_hits: cache.hits,
        cache_misses: cache.misses,
    };
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    println!("{json}");
    if let Some(out) = out {
        std::fs::write(&out, format!("{json}\n")).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    Ok(())
}
