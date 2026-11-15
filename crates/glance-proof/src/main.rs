//! The core's proof and verify binary. Not a product.
//!
//! ```sh
//! glance-proof <clip>... [--seconds 10] [--warmup 1] [--out results.json] [--log frames.csv]
//!               [--mute] [--no-audio] [--pace N] [--no-loop]
//! glance-proof scrub [--seconds 10] [--out report.json] <clip>
//! glance-proof media <file or folder>...
//! glance-proof verify [--every N] [--root <folder>] [--out report.json] <file or folder>...
//! glance-proof hapq [--every N] [--out report.json] <HAP Q clip>...
//! glance-proof gpu
//! ```
//! Without a subcommand: clips playing side by side in one egui window through the glance-core
//! crates (glance-media -> glance-playback / glance-audio -> glance-gpu -> glance-ui), **with their
//! sound** (the first audio track on the default output device; the picture follows it), with
//! frame timing logged. `--mute` plays the sound at gain 0 (the clock runs on exactly as audible:
//! for unattended measurements), `--no-audio` plays the picture on the wall clock, `--pace N`
//! takes a new frame only every N-th display refresh (N = 2 on a 120 Hz display is a 60 Hz
//! cadence), `--no-loop` stops at the last frame. `scrub` drags a clip's scrubber to a new random
//! frame every 8 ms without a window and times each request.
//! `verify` runs the 0-difference render harness without a window ([`verify`]); `hapq` measures
//! the Rust family's HAP Q picture against Swift Glance's convention ([`hapq`]); `gpu` prints the
//! adapter the family's device is on, as JSON.
//!
//! With `--seconds` the window closes by itself after measuring that long and the timing summary
//! is printed as JSON (and written to `--out`); `--log` gets one CSV row per display refresh.
//! Measurement starts after `--warmup` seconds (the window appearing and the display ramping up
//! to its full refresh rate are not playback). Quote numbers from a `--release` build only.

mod hapq;
mod verify;

use glance_gpu::{ClipView, Gpu, Presenter};
use glance_media::{AlphaRule, AlphaView, CacheSettings, Decoded};
use glance_playback::{AudioState, Cadence, ClipPlayer, Master, Samples, Sound, Summary};
use glance_ui::{App, Background, Checker, Host, Picture};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

mod scrub;

struct Args {
    clips: Vec<PathBuf>,
    seconds: Option<f64>,
    warmup: f64,
    out: Option<PathBuf>,
    log: Option<PathBuf>,
    mute: bool,
    sound: Sound,
    pace: u32,
    looping: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args { clips: Vec::new(), seconds: None, warmup: 1.0, out: None, log: None, mute: false, sound: Sound::Device, pace: 1, looping: true };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match a.as_str() {
            "--seconds" => args.seconds = Some(value("--seconds")?.parse::<f64>().map_err(|e| format!("--seconds: {e}"))?),
            "--warmup" => args.warmup = value("--warmup")?.parse::<f64>().map_err(|e| format!("--warmup: {e}"))?,
            "--out" => args.out = Some(value("--out")?.into()),
            "--log" => args.log = Some(value("--log")?.into()),
            "--mute" => args.mute = true,
            "--no-audio" => args.sound = Sound::Off,
            "--no-loop" => args.looping = false,
            "--pace" => args.pace = value("--pace")?.parse::<u32>().map_err(|e| format!("--pace: {e}"))?.max(1),
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            clip => args.clips.push(clip.into()),
        }
    }
    if args.clips.is_empty() {
        return Err(
            "usage: glance-proof <clip>... [--seconds N] [--warmup N] [--out results.json] [--log frames.csv] [--mute] [--no-audio] [--pace N] [--no-loop]"
                .into(),
        );
    }
    Ok(args)
}

/// One clip on screen.
struct Pane {
    path: PathBuf,
    player: ClipPlayer,
    view: ClipView,
    rule: AlphaRule,
    alpha: AlphaView,
    shown_alpha: Option<AlphaView>,
    picture: Option<Picture>,
    drawn: Option<egui::load::SizedTexture>,
    variant: Option<&'static str>,
    cadence: Cadence,
    /// Waiting for the frame (read-ahead hit, or a decode on a miss).
    fetch: Samples,
    /// CPU time of upload + conversion + present pass, per new frame.
    show: Samples,
    index: u32,
    /// The frame on screen (for the end-of-run picture check).
    last: Option<Arc<Decoded>>,
    error: Option<String>,
    /// Sound heard minus the shown frame's start, at the moment the picture is shown (ms), on
    /// the audio clock.
    av: Vec<f64>,
    /// When frame 0 came round again (loop wraps), for the loop period.
    wraps: Vec<Instant>,
    /// Refreshes since the last new frame was taken (`--pace`).
    since: u32,
    av_taken: bool,
}

impl Pane {
    fn open(path: &Path, gpu: &Gpu, args: &Args) -> Result<Pane, String> {
        let mut player = ClipPlayer::open_with(path, CacheSettings { capacity: 24, read_ahead: 8, threads: 4 }, args.sound)
            .map_err(|e| format!("{}: {e} ({})", path.display(), e.cause))?;
        player.clock_mut().set_muted(args.mute);
        player.clock_mut().set_looping(args.looping);
        let rule = glance_media::alpha_rule(player.info());
        let variant = player.info().dxv_variant.map(glance_media::dxv_variant_label);
        let alpha = if let AlphaRule::Stated(a) = rule { a } else { AlphaView::default() };
        Ok(Pane {
            path: path.to_path_buf(),
            player,
            view: ClipView::new(&gpu.device),
            rule,
            alpha,
            shown_alpha: None,
            picture: None,
            drawn: None,
            variant,
            cadence: Cadence::default(),
            fetch: Samples::default(),
            show: Samples::default(),
            index: 0,
            last: None,
            error: None,
            av: Vec::new(),
            wraps: Vec::new(),
            since: 0,
            av_taken: false,
        })
    }

    /// The A/V sample taken this refresh, for the CSV row (empty when none was).
    fn last_av(&self) -> String {
        if self.av_taken { self.av.last().map_or_else(String::new, |v| format!("{v:.3}")) } else { String::new() }
    }

    fn codec(&self) -> String {
        let label = &self.player.info().codec_name;
        self.variant.map_or(label.clone(), |v| format!("{label} {v}"))
    }

    /// Put the frame for the moment `shown_at` (when this refresh reaches the screen) on the GPU;
    /// returns (fetch ms, show ms) when a new frame went up. With `pace` N only every N-th refresh
    /// asks the clock.
    fn advance(&mut self, shown_at: Instant, pace: u32, presenter: &Presenter, host: &mut Host<'_>) -> Result<Option<(f64, f64)>, String> {
        self.av_taken = false;
        self.since = self.since.saturating_add(1);
        if self.since < pace && self.last.is_some() {
            return Ok(None);
        }
        self.since = 0;
        let shown = self.player.frame_at(shown_at).map_err(|e| e.to_string())?;
        if shown.index < self.index {
            self.wraps.push(shown_at);
        }
        self.index = shown.index;
        let clock = self.player.clock();
        if clock.master() == Master::Audio
            && !clock.is_paused()
            && let Some(heard) = clock.audio_position(shown_at)
        {
            let start = glance_playback::seconds(glance_playback::ticks(clock.time_of_frame(shown.index)));
            let length = clock.clip().timing.as_deref().map_or(0.0, |t| t.end_time().seconds());
            // Across the loop seam (frame 0 shown in the half frame before the wrap) compare with
            // the next loop's frame 0.
            let mut d = glance_playback::seconds(heard) - start;
            if d > length / 2.0 {
                d -= length;
            }
            self.av.push(d * 1000.0);
            self.av_taken = true;
        }
        let fresh = self.cadence.show(shown.index, self.player.info().frames);
        let mut timing = None;
        if fresh {
            let t = Instant::now();
            let picture = self.view.show(host.gpu, presenter, &shown.frame, self.alpha).map_err(|e| e.to_string())?.clone();
            let took = t.elapsed();
            self.fetch.push(shown.fetch);
            self.show.push(took);
            self.shown_alpha = Some(self.alpha);
            self.drawn = Some(host.picture(&mut self.picture, &picture));
            self.last = Some(shown.frame);
            timing = Some((shown.fetch.as_secs_f64() * 1000.0, took.as_secs_f64() * 1000.0));
        } else if self.shown_alpha != Some(self.alpha) {
            let picture = self.view.relook(host.gpu, presenter, self.alpha).map_err(|e| e.to_string())?.clone();
            self.shown_alpha = Some(self.alpha);
            self.drawn = Some(host.picture(&mut self.picture, &picture));
        }
        Ok(timing)
    }

    /// The picture on the GPU against the CPU reference of the same frame. `None` for frames with no
    /// CPU decode (native frames).
    fn check_picture(&self, gpu: &Gpu) -> Option<PictureCheck> {
        let (frame, picture) = (self.last.as_ref()?, self.view.picture()?);
        let cpu = glance_gpu::expected_picture(frame, self.alpha).ok()?;
        let gpu_rgba = glance_gpu::read_texture_rgba(&gpu.device, &gpu.queue, picture).ok()?;
        if gpu_rgba.len() != cpu.len() || cpu.is_empty() {
            return None;
        }
        let (mut max, mut sum, mut mean) = (0u8, 0u64, [0u64; 4]);
        for (g, c) in gpu_rgba.as_chunks::<4>().0.iter().zip(cpu.as_chunks::<4>().0) {
            for ((g, c), m) in g.iter().zip(c).zip(&mut mean) {
                let d = g.abs_diff(*c);
                max = max.max(d);
                sum += u64::from(d);
                *m += u64::from(*g);
            }
        }
        let pixels = (gpu_rgba.len() / 4) as f64;
        Some(PictureCheck { frame: frame.index(), max_diff: max, mean_diff: sum as f64 / (pixels * 4.0), mean_rgba: mean.map(|m| m as f64 / pixels) })
    }

    fn ui(&mut self, ui: &mut egui::Ui, salt: usize, background: Background, checker: &Checker) {
        let info = self.player.info();
        let name = self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        ui.label(egui::RichText::new(name).strong());
        ui.label(format!(
            "{}  ·  {}x{}  ·  {:.3} fps  ·  frame {} / {}",
            self.codec(),
            info.width,
            info.height,
            info.rate.as_f64(),
            self.index.saturating_add(1),
            info.frames
        ));
        ui.horizontal(|ui| match self.rule {
            AlphaRule::Choosable => {
                ui.label("Alpha Type");
                egui::ComboBox::from_id_salt(("alpha", salt)).selected_text(format!("{:?}", self.alpha)).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.alpha, AlphaView::Premultiplied, "Premultiplied");
                    ui.selectable_value(&mut self.alpha, AlphaView::Straight, "Straight");
                });
            }
            AlphaRule::Stated(a) => {
                ui.weak(format!("Alpha Type: {a:?} ({})", info.codec_name));
            }
            AlphaRule::Opaque => {
                ui.weak("No alpha");
            }
        });
        if let Some(e) = &self.error {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
        }
        if let Some(drawn) = self.drawn {
            glance_ui::video(ui, drawn, background, checker);
        }
    }
}

struct Proof {
    panes: Vec<Pane>,
    presenter: Presenter,
    background: Background,
    checker: Option<Checker>,
    started: Instant,
    seconds: Option<f64>,
    warmup: f64,
    /// When measuring began (the end of the warm-up).
    measured_from: Option<Instant>,
    out: Option<PathBuf>,
    log: Option<PathBuf>,
    /// Present-to-present time of the window.
    intervals: Samples,
    refresh_millihertz: Option<u32>,
    /// CSV rows, written at the end (no file I/O while playing).
    rows: Vec<String>,
    finished: bool,
    pace: u32,
}

/// The shown picture (GPU read-back) against the CPU decode of the same frame, in code values.
#[derive(serde::Serialize)]
struct PictureCheck {
    frame: u32,
    max_diff: u8,
    mean_diff: f64,
    /// Mean R, G, B, A of the picture (a blank picture reads 0 0 0 0).
    mean_rgba: [f64; 4],
}

#[derive(serde::Serialize)]
struct ClipReport {
    path: String,
    codec: String,
    width: u32,
    height: u32,
    frames: u32,
    clip_fps: f64,
    has_alpha: bool,
    alpha_view: String,
    /// Frames that reached the screen, and file frames jumped over.
    cadence: Cadence,
    shown_fps: f64,
    fetch: Option<Summary>,
    show: Option<Summary>,
    cache_hits: u64,
    cache_misses: u64,
    cache_waits: u64,
    prefetched: u64,
    worker_errors: u64,
    picture_vs_cpu: Option<PictureCheck>,
    error: Option<String>,
    /// The clock that drove the picture at the end, and the sound's state.
    master: Master,
    audio_state: String,
    audio: Option<AudioReport>,
    /// Sound heard minus the shown frame's start at the moment of showing, ms (audio clock).
    av_offset_ms: Option<Signed>,
    /// Time between two wraps onto frame 0, ms, and the clip's own length.
    loop_period_ms: Option<Signed>,
    clip_length_ms: f64,
}

/// The output device and the track, and what the heard time is built from.
#[derive(serde::Serialize)]
struct AudioReport {
    track_rate: u32,
    track_channels: u32,
    device_rate: u32,
    device_channels: u16,
    /// cpal's playback minus callback instant (buffer + device latency + safety offset), ms.
    cpal_reported_delay_ms: f64,
    buffer_frames: u64,
}

/// A signed distribution.
#[derive(serde::Serialize)]
struct Signed {
    count: usize,
    mean: f64,
    min: f64,
    p05: f64,
    p50: f64,
    p95: f64,
    max: f64,
}

fn signed(v: &[f64]) -> Option<Signed> {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let at = |p: f64| s.get(((p * s.len() as f64).ceil() as usize).clamp(1, s.len()).saturating_sub(1)).copied().unwrap_or(f64::NAN);
    Some(Signed { count: s.len(), mean: s.iter().sum::<f64>() / s.len() as f64, min: *s.first()?, p05: at(0.05), p50: at(0.5), p95: at(0.95), max: *s.last()? })
}

#[derive(serde::Serialize)]
struct Report {
    build: &'static str,
    warmup_seconds: f64,
    /// The measured span (after the warm-up).
    seconds: f64,
    display_refresh_hz: Option<f64>,
    presents: usize,
    present_interval: Option<Summary>,
    clips: Vec<ClipReport>,
}

impl Proof {
    fn report(&self, gpu: &Gpu) -> Report {
        let seconds = self.measured_from.map_or(0.0, |t| t.elapsed().as_secs_f64());
        let clips = self
            .panes
            .iter()
            .map(|p| {
                let (info, cache) = (p.player.info(), p.player.cache_stats());
                ClipReport {
                    path: p.path.display().to_string(),
                    codec: p.codec(),
                    width: info.width,
                    height: info.height,
                    frames: info.frames,
                    clip_fps: info.rate.as_f64(),
                    has_alpha: info.has_alpha,
                    alpha_view: if p.rule == AlphaRule::Opaque { "None".into() } else { format!("{:?}", p.alpha) },
                    cadence: p.cadence,
                    shown_fps: p.cadence.shown as f64 / seconds.max(1e-9),
                    fetch: p.fetch.summary(),
                    show: p.show.summary(),
                    cache_hits: cache.hits,
                    cache_misses: cache.misses,
                    cache_waits: cache.waits,
                    prefetched: cache.prefetched,
                    worker_errors: cache.worker_errors,
                    picture_vs_cpu: p.check_picture(gpu),
                    error: p.error.clone(),
                    master: p.player.clock().master(),
                    audio_state: match p.player.clock().audio_state() {
                        AudioState::Playing { sample_rate, channels } => format!("playing ({sample_rate} Hz, {channels} ch)"),
                        AudioState::None { reason } => format!("none ({reason})"),
                        AudioState::Failed { reason } => format!("failed ({reason})"),
                    },
                    audio: p.player.clock().audio_description().map(|a| AudioReport {
                        track_rate: a.track.rate,
                        track_channels: a.track.channels,
                        device_rate: a.device.rate,
                        device_channels: a.device.channels,
                        cpal_reported_delay_ms: a.reported_delay_nanos as f64 / 1e6,
                        buffer_frames: a.buffer_frames,
                    }),
                    av_offset_ms: signed(&p.av),
                    loop_period_ms: signed(&p.wraps.windows(2).map(|w| (w[1] - w[0]).as_secs_f64() * 1000.0).collect::<Vec<_>>()),
                    clip_length_ms: glance_playback::seconds(glance_playback::ticks(
                        p.player.clock().clip().timing.as_deref().map_or(glance_media::MediaTime::new(0, 1), |t| t.end_time()),
                    )) * 1000.0,
                }
            })
            .collect();
        Report {
            build: if cfg!(debug_assertions) { "debug" } else { "release" },
            warmup_seconds: self.warmup,
            seconds,
            display_refresh_hz: self.refresh_millihertz.map(|m| f64::from(m) / 1000.0),
            presents: self.intervals.len(),
            present_interval: self.intervals.summary(),
            clips,
        }
    }

    fn finish(&mut self, gpu: &Gpu) {
        self.finished = true;
        let json = serde_json::to_string_pretty(&self.report(gpu)).unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
        println!("{json}");
        if let Some(out) = &self.out
            && let Err(e) = std::fs::write(out, format!("{json}\n"))
        {
            eprintln!("glance-proof: {}: {e}", out.display());
        }
        if let Some(log) = &self.log {
            let header: String = (0..self.panes.len()).map(|i| format!(",clip{i}_frame,clip{i}_fetch_ms,clip{i}_show_ms,clip{i}_av_ms")).collect();
            let body = format!("t_ms,present_interval_ms{header}\n{}\n", self.rows.join("\n"));
            if let Err(e) = std::fs::write(log, body) {
                eprintln!("glance-proof: {}: {e}", log.display());
            }
        }
    }
}

impl App for Proof {
    fn frame(&mut self, ui: &mut egui::Ui, host: &mut Host<'_>) {
        let now = Instant::now();
        self.refresh_millihertz = host.refresh_millihertz.or(self.refresh_millihertz);
        if self.measured_from.is_none() && (now - self.started).as_secs_f64() >= self.warmup {
            self.measured_from = Some(now);
            self.rows.clear();
            for pane in &mut self.panes {
                (pane.cadence, pane.fetch, pane.show) = Default::default();
                (pane.av, pane.wraps) = Default::default();
            }
        } else if self.measured_from.is_some()
            && let Some(d) = host.last_interval
        {
            self.intervals.push(d);
        }
        let mut row = format!("{:.3},{:.3}", (now - self.started).as_secs_f64() * 1000.0, host.last_interval.map_or(0.0, |d| d.as_secs_f64() * 1000.0));
        // This refresh reaches the screen about one refresh from now (FIFO, the frame drawn now is
        // presented at the next vertical blank; up to two with a frame queued).
        let refresh = self.refresh_millihertz.filter(|&m| m > 0).map_or(Duration::from_micros(8_333), |m| Duration::from_secs_f64(1000.0 / f64::from(m)));
        let shown_at = now + refresh;
        for pane in &mut self.panes {
            match pane.advance(shown_at, self.pace, &self.presenter, host) {
                Ok(Some((fetch, show))) => row.push_str(&format!(",{},{fetch:.3},{show:.3},{}", pane.index, pane.last_av())),
                Ok(None) => row.push_str(&format!(",{},,,{}", pane.index, pane.last_av())),
                Err(e) => {
                    pane.error = Some(e);
                    row.push_str(",,,,");
                }
            }
        }
        self.rows.push(row);

        let elapsed = self.started.elapsed().as_secs_f64();
        let fps = host.last_interval.map_or(0.0, |d| 1.0 / d.as_secs_f64().max(1e-9));
        ui.horizontal(|ui| {
            ui.label(format!("glance-core proof  ·  {elapsed:.1} s  ·  window {fps:.1} fps  ·  Background"));
            ui.selectable_value(&mut self.background, Background::Black, "Black");
            ui.selectable_value(&mut self.background, Background::Checkerboard, "Checkerboard");
        });
        ui.separator();
        let (background, checker) = (self.background, &*self.checker.get_or_insert_with(|| Checker::new(ui.ctx())));
        ui.columns(self.panes.len(), |columns| {
            for (i, (column, pane)) in columns.iter_mut().zip(&mut self.panes).enumerate() {
                pane.ui(column, i, background, checker);
            }
        });

        let measured = self.measured_from.map_or(0.0, |t| t.elapsed().as_secs_f64());
        if self.seconds.is_some_and(|s| measured >= s) && !self.finished {
            self.finish(host.gpu);
        }
    }

    fn finished(&self) -> bool {
        self.finished
    }
}

/// `verify` and `hapq`: options, then files; the report goes to stdout or `--out` as JSON.
fn tool(name: &str, rest: &[String]) -> Result<(), String> {
    let (mut every, mut out, mut root, mut paths) = (1u32, None::<PathBuf>, None::<PathBuf>, Vec::<PathBuf>::new());
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match a.as_str() {
            "--every" => every = value("--every")?.parse::<u32>().map_err(|e| format!("--every: {e}"))?.max(1),
            "--out" => out = Some(value("--out")?.into()),
            "--root" => root = Some(value("--root")?.into()),
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            path => paths.push(path.into()),
        }
    }
    if paths.is_empty() {
        return Err(format!("usage: glance-proof {name} [--every N] [--root <folder>] [--out report.json] <file or folder>..."));
    }
    let json = if name == "verify" {
        serde_json::to_string_pretty(&verify::run(&paths, root.as_deref(), every)?)
    } else {
        let rows =
            verify::movies(&paths).iter().map(|p| hapq::measure(p, every).map_err(|e| format!("{}: {e}", p.display()))).collect::<Result<Vec<_>, _>>()?;
        serde_json::to_string_pretty(&rows)
    }
    .map_err(|e| e.to_string())?;
    match out {
        Some(out) => std::fs::write(&out, format!("{json}\n")).map_err(|e| format!("{}: {e}", out.display())),
        None => {
            println!("{json}");
            Ok(())
        }
    }
}

fn main() -> std::process::ExitCode {
    let all: Vec<String> = std::env::args().skip(1).collect();
    if all.first().map(String::as_str) == Some("gpu") {
        return match glance_gpu::open_gpu() {
            Ok(gpu) => {
                let (info, limits) = (gpu.adapter.get_info(), gpu.device.limits());
                let report = serde_json::json!({
                    "adapter": info.name,
                    "backend": format!("{:?}", info.backend),
                    "device_type": format!("{:?}", info.device_type),
                    "driver": info.driver_info,
                    "bc_textures": gpu.has_bc(),
                    "max_texture_dimension_2d": limits.max_texture_dimension_2d,
                });
                println!("{report:#}");
                std::process::ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("glance-proof: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    if all.first().map(String::as_str) == Some("media") {
        for path in verify::movies(&all.iter().skip(1).map(PathBuf::from).collect::<Vec<_>>()) {
            let line = match glance_media::route(&path) {
                Ok(c) => serde_json::json!({
                    "file": path.file_name().map(|n| n.to_string_lossy().into_owned()),
                    "route": format!("{:?}", c.route),
                    "codec": c.info.codec_name,
                    "variant": c.info.dxv_variant.map(glance_media::dxv_variant_label),
                    "width": c.info.width, "height": c.info.height,
                    "fps": c.info.rate.as_f64(), "frames": c.info.frames,
                    "audio": c.info.audio.iter().map(|a| serde_json::json!({"codec": a.codec_name, "channels": a.channels, "rate": a.sample_rate, "frames": a.frames, "error": a.error})).collect::<Vec<_>>(),
                }),
                Err(e) => serde_json::json!({"file": path.file_name().map(|n| n.to_string_lossy().into_owned()), "error": e.message}),
            };
            println!("{line}");
        }
        return std::process::ExitCode::SUCCESS;
    }
    if all.first().map(String::as_str) == Some("scrub") {
        return match scrub::run(all.get(1..).unwrap_or_default()) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("glance-proof: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    if let Some(name @ ("verify" | "hapq")) = all.first().map(String::as_str) {
        return match tool(name, all.get(1..).unwrap_or_default()) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("glance-proof: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("glance-proof: {e}");
            return std::process::ExitCode::from(2);
        }
    };
    let result = glance_ui::run("glance-core proof", (1600.0, 620.0), |gpu| {
        let panes = args.clips.iter().map(|c| Pane::open(c, gpu, &args)).collect::<Result<Vec<_>, _>>()?;
        Ok(Proof {
            panes,
            presenter: Presenter::new(&gpu.device),
            background: Background::Checkerboard,
            checker: None,
            started: Instant::now(),
            seconds: args.seconds,
            warmup: args.warmup,
            measured_from: None,
            out: args.out,
            log: args.log,
            intervals: Samples::default(),
            refresh_millihertz: None,
            rows: Vec::new(),
            finished: false,
            pace: args.pace,
        })
    });
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("glance-proof: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
