//! Frame `n` is frame `n` on every path (docs/02-PARITY.md, core "Time" and Glance "Transport"):
//! - the player's frame after shuffled seeks equals the sequential decode of that frame, for the
//!   DXV and HAP fixtures and the OS-decoder fixtures;
//! - Swift Glance's eight frame-timing fixtures (frame N has luma 5 N; B-frames at 23.976 / 25 /
//!   29.97 / 30 / 60 fps, jittered H.264 and ProRes 4444): a seek to N shows N, 40 of 40 each, and
//!   the clock, run on the wall clock, shows N from N's real start (`Clip::from_timing`), and wraps
//!   the loop onto frame 0 exactly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use glance_media::{CacheSettings, FrameSource, route};
use glance_playback::{ClipPlayer, Sound};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "../../../tests/support/movie.rs"]
mod support;

fn fixtures() -> PathBuf {
    support::fixtures()
}

const CACHE: CacheSettings = CacheSettings { capacity: 8, read_ahead: 2, threads: 2 };

/// A fixed shuffle of `0..n` (a 64-bit LCG; no dependency).
fn shuffled(n: u32) -> Vec<u32> {
    let mut v: Vec<u32> = (0..n).collect();
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    for i in (1..v.len()).rev() {
        x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        v.swap(i, (x >> 33) as usize % (i + 1));
    }
    v
}

/// The GPU for native frames (YUV surfaces have no CPU read path): `None` skips, unless
/// `GLANCE_REQUIRE_GPU=1`.
struct Gpu {
    gpu: glance_gpu::Gpu,
    presenter: glance_gpu::Presenter,
}

fn gpu() -> Option<Gpu> {
    let gpu = glance_gpu::verify::test_gpu().unwrap()?;
    let presenter = glance_gpu::Presenter::new(&gpu.device);
    Some(Gpu { gpu, presenter })
}

/// The frame's RGBA: the CPU decode for block frames (through the oracle source), the picture read
/// back from the GPU for native frames.
fn rgba(oracle: &dyn FrameSource, frame: &glance_media::Decoded, g: Option<&Gpu>) -> Vec<u8> {
    match frame {
        glance_media::Decoded::Native(_) => {
            let g = g.expect("native frames need the GPU");
            let mut view = glance_gpu::ClipView::new(&g.gpu.device);
            let picture = view.show(&g.gpu, &g.presenter, frame, glance_media::AlphaView::Premultiplied).unwrap().clone();
            glance_gpu::read_texture_rgba(&g.gpu.device, &g.gpu.queue, &picture).unwrap()
        }
        _ => oracle.cpu_rgba(frame).unwrap(),
    }
}

fn shuffled_seeks_equal_sequential_decode(path: &Path, g: Option<&Gpu>) -> u32 {
    let mut player = ClipPlayer::open_with(path, CACHE, Sound::Off).unwrap();
    let mut oracle = route(path).unwrap().open().unwrap();
    let frames = player.clock().clip().frames;
    let sequential: Vec<Vec<u8>> = (0..frames).map(|n| oracle.decode(n).map(|d| rgba(oracle.as_ref(), &d, g)).unwrap()).collect();
    player.clock_mut().pause();
    let now = Instant::now();
    for n in shuffled(frames) {
        player.clock_mut().seek(n);
        let shown = player.frame_at(now).unwrap();
        assert_eq!(shown.index, n, "{}", path.display());
        assert!(
            rgba(oracle.as_ref(), &shown.frame, g) == sequential[n as usize],
            "{}: frame {n} after a seek differs from the sequential decode",
            path.display()
        );
    }
    frames
}

#[test]
fn shuffled_seeks_equal_sequential_decode_on_every_path() {
    let g = gpu();
    let mut names: Vec<PathBuf> = support::block_fixtures();
    if cfg!(target_os = "macos") && g.is_some() {
        for n in ["tiles_h264.mp4", "tiles_hevc.mp4", "tiles_prores4444.mov", "tiles_mjpeg.mov"] {
            names.push(fixtures().join(n));
        }
    }
    let mut total = 0;
    for path in &names {
        total += shuffled_seeks_equal_sequential_decode(path, g.as_ref());
    }
    eprintln!("{} clips, {total} frames sought in shuffled order", names.len());
    assert!(total > 300, "{total}");
}

const TIMING: [&str; 8] = [
    "frames_h264_23.976.mov",
    "frames_h264_25.mov",
    "frames_h264_29.97.mov",
    "frames_h264_30.mov",
    "frames_h264_60.mov",
    "frames_h264_jitter25_early.mov",
    "frames_h264_jitter25_late.mov",
    "frames_prores4444_jitter30_late.mov",
];

/// Mean luma-ish level of a frame (the mean of R, G and B), in code values.
fn level(rgba: &[u8]) -> f64 {
    let sum: u64 = rgba.as_chunks::<4>().0.iter().map(|p| u64::from(p[0]) + u64::from(p[1]) + u64::from(p[2])).sum();
    sum as f64 / (rgba.len() / 4 * 3) as f64
}

#[test]
fn swift_glances_frame_timing_fixtures_land_on_frame_n() {
    if cfg!(not(target_os = "macos")) {
        eprintln!("SKIP: no OS decoder on this platform yet");
        return;
    }
    let Some(g) = gpu() else { return };
    for name in TIMING {
        let path = fixtures().join("frame-timing").join(name);
        let mut player = ClipPlayer::open_with(&path, CACHE, Sound::Off).unwrap();
        let oracle = route(&path).unwrap().open().unwrap();
        let clip = player.clock().clip().clone();
        assert_eq!(clip.frames, 40, "{name}");
        let table = clip.timing.as_deref().is_some_and(|t| t.has_table());
        // Seeks, shuffled: frame N's level is the N-th distinct step (luma 5 N), and N comes back.
        player.clock_mut().pause();
        let now = Instant::now();
        let mut levels = vec![0.0; 40];
        for n in shuffled(40) {
            player.clock_mut().seek(n);
            let shown = player.frame_at(now).unwrap();
            assert_eq!(shown.index, n);
            levels[n as usize] = level(&rgba(oracle.as_ref(), &shown.frame, Some(&g)));
        }
        let steps = levels.windows(2).filter(|w| w[1] > w[0] + 2.0).count();
        assert_eq!(steps, 39, "{name}: levels {levels:?}");
        // The clock on the wall clock: from frame 0, frame N shows from its real start (1 us after)
        // and N - 1 shows 1 us before it.
        player.clock_mut().seek(0);
        player.clock_mut().play();
        let t0 = Instant::now();
        assert_eq!(player.clock_mut().tick(t0), 0);
        let z = player.clock().time_of_frame(0).seconds();
        let starts: Vec<Duration> = (0..40).map(|n| Duration::from_secs_f64(player.clock().time_of_frame(n).seconds() - z)).collect();
        let mut wrong = Vec::new();
        for n in 1..40u32 {
            let s = starts[n as usize];
            let before = player.clock_mut().tick(t0 + s - Duration::from_micros(2));
            let after = player.clock_mut().tick(t0 + s + Duration::from_micros(2));
            if (before, after) != (n - 1, n) {
                wrong.push((n, before, after));
            }
        }
        assert!(wrong.is_empty(), "{name} (table: {table}): {wrong:?}");
        // The loop wraps onto frame 0 exactly at the clip's end.
        let end = clip.timing.as_deref().map(|t| t.end_time().seconds() - player.clock().time_of_frame(0).seconds()).unwrap();
        assert_eq!(player.clock_mut().tick(t0 + Duration::from_secs_f64(end) - Duration::from_micros(2)), 39, "{name}");
        assert_eq!(player.clock_mut().tick(t0 + Duration::from_secs_f64(end) + Duration::from_micros(2)), 0, "{name}");
        // Negative control: on the rate grid alone (no table) a jittered clip misses real starts.
        let mut grid = glance_playback::Clock::new(glance_media::transport::Clip::new(40, clip.rate), Err(glance_playback::AudioError::NoAudioTrack));
        let g0 = Instant::now();
        grid.tick(g0);
        let grid_wrong = (1..40u32)
            .filter(|&n| {
                let s = starts[n as usize];
                (grid.tick(g0 + s - Duration::from_micros(2)), grid.tick(g0 + s + Duration::from_micros(2))) != (n - 1, n)
            })
            .count();
        if name.contains("jitter") {
            assert!(grid_wrong > 0, "{name}: the grid should miss a jittered start");
        }
        eprintln!("{name}: 40 of 40 by seek, 39 of 39 frame starts by the clock, loop onto 0 (table: {table}); the grid alone misses {grid_wrong}");
    }
}
