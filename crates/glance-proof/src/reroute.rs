//! `glance-proof reroute`: the output device's rate changes while a clip's sound plays, and the
//! sound keeps its speed.
//!
//! The clip's first audio track plays muted (gain 0: nothing is heard, the device callback runs as
//! audible) on the default output. After `--switch-at` seconds another thread changes the default
//! output's hardware rate to `--to` Hz, the way any app can: cpal 0.18 sets a device's physical
//! format when it builds a stream at another rate, so a silent stream is built at `--to` and
//! dropped. At the end the device is put back at the rate it had. Every 50 ms the run records the
//! frames the clock's callback has played; the **speed** of a stretch is those frames per wall
//! second over the rate the track was resampled for — 1.000 is right. `--no-follow` never calls
//! `AudioClock::follow_device` (the negative control: the C2 behaviour).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use glance_audio::AudioClock;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(serde::Serialize)]
struct Sample {
    t: f64,
    rendered: u64,
    /// The rate the playing samples were resampled for.
    prepared_rate: u32,
    position_s: Option<f64>,
}

#[derive(serde::Serialize)]
struct Report {
    build: &'static str,
    clip: String,
    follow: bool,
    device_rate_before: u32,
    switched_to: u32,
    switched_at_s: f64,
    restored_to: Option<u32>,
    reroutes: Vec<String>,
    /// Speed over [0.5 s, switch) and over [switch + 1 s, end).
    speed_before: Option<f64>,
    speed_after: Option<f64>,
    xruns: u64,
    failed: Option<String>,
    /// `follow_device` call times, ms: the calls that found nothing changed, and every call.
    follow_quiet_ms: Option<glance_playback::Summary>,
    follow_max_ms: f64,
    samples: Vec<Sample>,
}

/// Build a silent stream on the default output at `rate` Hz (cpal sets the hardware there), keep it
/// running a moment, drop it.
fn set_default_rate(rate: u32) -> Result<(), String> {
    let device = cpal::default_host().default_output_device().ok_or("no default output")?;
    let channels = device.default_output_config().map_err(|e| e.to_string())?.channels();
    let config = cpal::StreamConfig { channels, sample_rate: rate, buffer_size: cpal::BufferSize::Default };
    let stream = device
        .build_output_stream::<f32, _, _>(config, |out: &mut [f32], _: &cpal::OutputCallbackInfo| out.fill(0.0), |_| {}, None)
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    std::thread::sleep(Duration::from_millis(300));
    Ok(())
}

fn speed(samples: &[Sample], from: f64, to: f64) -> Option<f64> {
    let inside: Vec<&Sample> = samples.iter().filter(|s| s.t >= from && s.t < to).collect();
    let (a, b) = (inside.first()?, inside.last()?);
    if b.t <= a.t || a.prepared_rate != b.prepared_rate {
        return None;
    }
    Some((b.rendered.saturating_sub(a.rendered)) as f64 / (b.t - a.t) / f64::from(b.prepared_rate))
}

pub fn run(rest: &[String]) -> Result<(), String> {
    let (mut seconds, mut switch_at, mut to, mut follow, mut out, mut clip) = (10.0f64, 4.0f64, 48_000u32, true, None::<PathBuf>, None::<PathBuf>);
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or_else(|| format!("{name} needs a value"));
        match a.as_str() {
            "--seconds" => seconds = value("--seconds")?.parse().map_err(|e| format!("--seconds: {e}"))?,
            "--switch-at" => switch_at = value("--switch-at")?.parse().map_err(|e| format!("--switch-at: {e}"))?,
            "--to" => to = value("--to")?.parse().map_err(|e| format!("--to: {e}"))?,
            "--no-follow" => follow = false,
            "--out" => out = Some(value("--out")?.into()),
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            path => clip = Some(path.into()),
        }
    }
    let clip = clip.ok_or("usage: glance-proof reroute [--seconds N] [--switch-at N] [--to HZ] [--no-follow] [--out r.json] <clip with sound>")?;
    let routed = glance_media::route(&clip).map_err(|e| e.message)?;
    let before = glance_audio::default_output().map_err(|e| e.to_string())?.1.rate;
    let mut audio = AudioClock::open(&clip, &routed.info.audio, true).map_err(|e| e.to_string())?;
    audio.set_muted(true);
    audio.play();
    let start = Instant::now();
    let mut samples = Vec::new();
    let mut reroutes = Vec::new();
    let mut switcher = None;
    let mut quiet = glance_playback::Samples::default();
    let mut follow_max = 0.0f64;
    while start.elapsed().as_secs_f64() < seconds {
        let t = start.elapsed().as_secs_f64();
        if switcher.is_none() && t >= switch_at {
            switcher = Some(std::thread::spawn(move || set_default_rate(to)));
        }
        if follow {
            let called = Instant::now();
            let result = audio.follow_device();
            let took = called.elapsed();
            follow_max = follow_max.max(took.as_secs_f64() * 1e3);
            if matches!(result, Ok(None)) && (t - switch_at).abs() > 1.0 {
                quiet.push(took);
            }
            match result {
                Ok(Some(r)) => reroutes.push(format!(
                    "t {:.3} s: {} Hz {} ch -> {} Hz {} ch ({}), took {:.1} ms",
                    start.elapsed().as_secs_f64(),
                    r.from.rate,
                    r.from.channels,
                    r.to.rate,
                    r.to.channels,
                    r.why,
                    r.took.as_secs_f64() * 1e3
                )),
                Ok(None) => {}
                Err(e) => reroutes.push(format!("t {t:.3} s: failed: {e}")),
            }
        }
        let (_, _, rendered) = audio.device_counts();
        samples.push(Sample { t, rendered, prepared_rate: audio.device().rate, position_s: audio.position().map(|p| p.seconds()) });
        std::thread::sleep(Duration::from_millis(50));
    }
    let switched = switcher.map(|h| h.join().map_err(|_| "rate thread panicked".to_string()).and_then(|r| r)).transpose()?;
    let failed = audio.failed();
    let (_, xruns, _) = audio.device_counts();
    drop(audio);
    // Put the device back at the rate it had.
    let now = glance_audio::default_output().map(|d| d.1.rate).ok();
    let restored_to = if now != Some(before) { set_default_rate(before).ok().map(|()| before) } else { None };
    let report = Report {
        build: if cfg!(debug_assertions) { "debug" } else { "release" },
        clip: clip.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        follow,
        device_rate_before: before,
        switched_to: if switched.is_some() { to } else { 0 },
        switched_at_s: switch_at,
        restored_to,
        reroutes,
        speed_before: speed(&samples, 0.5, switch_at),
        speed_after: speed(&samples, switch_at + 1.0, seconds),
        xruns,
        failed,
        follow_quiet_ms: quiet.summary(),
        follow_max_ms: follow_max,
        samples,
    };
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    if let Some(path) = out {
        std::fs::write(&path, &json).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let mut summary = serde_json::to_value(&report).map_err(|e| e.to_string())?;
    if let Some(o) = summary.as_object_mut() {
        o.remove("samples");
    }
    println!("{summary:#}");
    Ok(())
}
