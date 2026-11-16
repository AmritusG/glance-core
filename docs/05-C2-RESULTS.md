# C2 results — time and sound (macOS)

2026-11-15, core wave C2, on the core's home machine (<host>: Apple M5 Max, macOS 27.0.1, built-in
display at 120 Hz, default output device at 44.1 kHz stereo). Release builds only
(`target/release/glance-proof`, `"build": "release"` in every JSON). Arena was quit by Amrit for the
runs; **Blender stayed open** (about 11 % CPU; Amrit: "Keep Blender open"). Raw data:
`docs/data/c2-runs/` (one JSON per run; the runner is `tools/c2-measure.sh`). Test media: Amrit's
folder `<testfiles>` (read-only; files named
below), and two 2160p DXV clips made for this wave (section 6), kept out of the repo.

## 1. No clip frame skipped at 1x

Each run: one clip in the proof window (`glance-proof <clip> --seconds 15 --warmup 1 --pace N`),
15 s measured after 1 s warm-up. *Skipped* = file frames jumped over between two consecutive
showings, of *shown* showings. Sound runs use `--mute` (gain 0: the audio clock runs exactly as when
audible — the rule "gain never moves the clock", section 4); *sound off* = `--no-audio`.

**60 Hz** was not set on the display (Amrit: "Simulate at 120 Hz"): `--pace 2` asks the clock for a
new frame only every second refresh of the 120 Hz display — a 60 Hz cadence, the picture still
presented at 120 Hz.

| Run | Clip | 120 Hz pass 1 | 120 Hz pass 2 | 60 Hz cadence pass 1 | 60 Hz cadence pass 2 | after the seam fix (120 / 60) |
|---|---|---|---|---|---|---|
| hap1080-hapq | `Metric_01_HapM.mov` — HAP Q Alpha, 1920x1080, 25 fps, no audio | 0 / 376 | 0 / 376 | 0 / 376 | 0 / 377 | — |
| hap1080-hap | `glenc_hap1_1920x1080_chunks4.mov` — HAP, 1920x1080, 30 fps, no audio | 0 / 452 | 0 / 451 | **1** / 450 | 0 / 451 | — |
| hap2160-hap5 | `glenc_hap5_alpha_real_chunks4.mov` — HAP Alpha, 3840x2160, 30 fps, no audio | 0 / 451 | 0 / 451 | 0 / 451 | 0 / 451 | — |
| hap2160-hapqa-pcm | `glenc_hapm_LEAKTEST_4k.mov` — HAP Q Alpha, 3840x2160, 30 fps, **PCM, audio clock** | 0 / 451 | 0 / 451 | **1** / 450 | 0 / 450 | 0 / 452 · 0 / 451 |
| hap2160-hapqa-pcm-soundoff | the same, sound off | 0 / 452 | 0 / 451 | 0 / 451 | 0 / 451 | — |
| hap288-aac | `hap-testclips/test-hap1-audio.mov` — HAP, 512x288, 30 fps, **AAC, audio clock** | 0 / 452 | 0 / 452 | **1** / 450 | 0 / 452 | 0 / 452 · 0 / 451 |
| dxv1080-dxt5 | `DXT5-DXV3.mov` — DXV3 Normal Quality With Alpha, 1920x1080, 30 fps, no audio | 0 / 451 | 0 / 452 | 0 / 451 | 0 / 451 | — |
| dxv1080-ycg6 | `YCG6-DXV3.mov` — DXV3 High Quality, 1920x1080, 30 fps, no audio | 0 / 451 | 0 / 451 | **3** / 448 | 0 / 451 | — |
| dxv1080-yg10-pcm | `hap-testclips/std_film_h264_audio.mov` — DXV3 High Quality With Alpha, 1920x1080, 25 fps, **PCM, audio clock** | 0 / 376 | 0 / 376 | 0 / 376 | 0 / 376 | 0 / 376 · 0 / 376 |
| dxv1080-yg10-pcm-soundoff | the same, sound off | 0 / 376 | 0 / 376 | 0 / 377 | 0 / 376 | — |
| dxv1080-aac | `bad-audio-fixtures/good-audio.mov` — DXV3 High Quality With Alpha, 1920x1080, 25 fps, **AAC, audio clock** | 0 / 376 | 0 / 376 | 0 / 376 | 0 / 376 | 0 / 376 · 0 / 376 |
| dxv2160-ycg6 | `dxv_ycg6_3840x2160.mov` (made, section 6) — DXV3 High Quality, 3840x2160, 30 fps, no audio | 0 / 451 | 0 / 451 | 0 / 451 | 0 / 451 | — |
| dxv2160-dxt1 | `dxv_dxt1_3840x2160.mov` (made) — DXV3 Normal Quality, 3840x2160, 30 fps, no audio | 0 / 452 | 0 / 451 | 0 / 450 | 0 / 450 | — |

Raw: `docs/data/c2-runs/pass1/`, `pass2/`, `pass3-audio/` (`c2-<run>-p<pace>.json`).

- **120 Hz: 0 skipped in all 30 runs** (13 + 13 + 4 after the seam fix), 1080p and 2160p, HAP and
  DXV, with and without sound (and in the drift runs of section 3 after the fix).
- **60 Hz cadence: 0 skipped in 26 of 30 runs; pass 1 had 4 runs with 1, 1, 1 and 3 skipped**, all
  30 fps clips (no slack: one new frame per two samples). Pass 2 of the same 13 runs and the 4
  audio runs of pass 3: 0. Two reruns of the worst one (`YCG6-DXV3.mov`, logged per refresh): 0 and 0
  (`docs/data/c2-runs/diagnose/c2-rerun-dxv1080-ycg6-p2-{1,2}.{json,csv}`).
- Why a frame can be skipped at all: on this machine today the window's presents come in bursts —
  p50 8.24–8.33 ms but p95 about 16.7 ms in 65 of the 67 window runs (max 41.1 ms), a 16.7 ms interval followed by a short
  one (in the 62 s PCM log: 1–2 ms in 51 %, under 0.5 ms in 31 %, about 8 ms in 15 %) (`docs/data/c2-runs/pass1` and the CSV logs below). The same with the C1 binary
  (tag v0.1.0) run back to back on the same clip (p95 16.67 / 16.65 ms against 16.69 / 16.68 ms for
  this wave's build), so it is not this wave's code (`docs/data/c2-runs/diagnose/c2-ab-v010-vs-c2-*.json`); W0 measured p95 8.5 ms on a quieter machine
  (`docs/03-W0-RESULTS.md`). At a 60 Hz cadence two late presents in a row give a 33 ms gap, a whole
  30 fps frame. The frame itself is never the bottleneck: fetch max 0.038 ms (read-ahead), upload +
  conversion + Alpha Type pass p99 at most 3.26 ms (2160p HAP Q Alpha), and the picture read back
  equals the CPU decode, **max difference 0**, in every run that has a CPU reference (all but the
  H.264 FLAC run, whose frames come from the OS decoder).
- Not covered, no such clip: **DXV 2160p with audio** and **HAP 1080p with audio** (none in the test
  folder; the made 2160p DXV clips have no sound track). The audio clock is measured on 2160p HAP Q
  Alpha (PCM), 1080p DXV (PCM and AAC) and 512x288 HAP (AAC).

### 1a. 60 Hz on a real display (2026-11-16, v0.3.3)

Amrit: "60 Hz run can be done now." Run on the same machine (macOS 27.0.1) on **Amrit's projector,
Panasonic PT-RZ570** (CoreGraphics display 3, vendor 13481 / model 53534, winit "Monitor #53534";
1920 x 1200 points = pixels, scale 1; mode **60.00 Hz** as macOS reports it), connected to the right
of the built-in display; its settings were not touched (`docs/data/v033-60hz/displays.txt`,
`system-profiler-displays.txt`). The window was opened on it with the proof's new `--display` option
(moved there on the first frame, within the 1 s warm-up; every JSON records `"display": "Monitor
#53534"`, `"display_refresh_hz": 60.0`), **pace 1** (a new frame asked for at every refresh of the
60 Hz display; no simulation). Same 13 runs, clips, durations and two passes as above
(`tools/c2-measure.sh <synth> <out> 1 "Monitor #53534"`), release build, plus one CSV row per refresh.
Raw: `docs/data/v033-60hz/pass1/`, `pass2/`, `summary.txt`.

Load (`top`-style samples every 5 s, 84 samples, `load.txt`): Arena open the whole time (25–46 % CPU;
untouched by this brief); two long-running Python processes up to 880 % (39 samples, 21:50–21:57);
another session's Rust builds (rustc up to 624 %, 21:50:42–21:54:29; clang) and its `tl-play` playback
runs again (21:53:23–28 and 21:55:24–21:57:30, after the wait for the first one to end); another project's app
`tkapp_check` up to 170 % and `ui_crawl` 97 %; a Time Machine backup (`backupd` up to 122 %,
21:51–21:55); Spotlight, photo analysis, Chrome. Load beyond Arena sat in nearly every run. Not an
idle machine; nothing of it was started by the measuring.

| Run (as above) | pass 1 skipped / shown | presents p95 / max (ms) | pass 2 skipped / shown | presents p95 / max (ms) |
|---|---|---|---|---|
| hap1080-hapq-noaudio | 0 / 377 | 17.18 / 24.00 | 0 / 377 | 17.39 / 23.04 |
| hap1080-hap-noaudio | 0 / 452 | 17.31 / 23.74 | 0 / 451 | 22.16 / 27.56 |
| hap2160-hap5-noaudio | 0 / 452 | 17.21 / 24.46 | 0 / 452 | 17.22 / 21.10 |
| hap2160-hapqa-pcm (audio clock) | 0 / 451 | 17.70 / 22.23 | 0 / 452 | 17.31 / 20.61 |
| hap2160-hapqa-pcm-soundoff | 0 / 452 | 17.43 / 24.01 | 0 / 452 | 17.32 / 21.30 |
| hap288-aac (audio clock) | 0 / 452 | 17.35 / 25.06 | 0 / 452 | 17.13 / 23.65 |
| dxv1080-dxt5-noaudio | 0 / 451 | 17.41 / 22.93 | 0 / 451 | 17.08 / 21.11 |
| dxv1080-ycg6-noaudio | 0 / 452 | 17.38 / 21.61 | 0 / 451 | 17.40 / 25.04 |
| dxv1080-yg10-pcm (audio clock) | 0 / 377 | 17.14 / 23.89 | 0 / 376 | 17.47 / 23.31 |
| dxv1080-yg10-pcm-soundoff | 0 / 376 | 17.48 / 21.74 | 0 / 376 | 17.70 / 22.95 |
| dxv1080-aac (audio clock) | 0 / 376 | 17.30 / 21.25 | 0 / 376 | 17.28 / 24.16 |
| dxv2160-ycg6-noaudio | 0 / 451 | 18.80 / 25.85 | 0 / 452 | 20.18 / 25.10 |
| dxv2160-dxt1-noaudio | 0 / 451 | 18.07 / 24.66 | 0 / 451 | 17.14 / 23.22 |

- **0 skipped in all 26 runs** (11,139 frames shown), 1080p and 2160p, HAP and DXV, with and without
  sound; the picture read back equals the CPU decode, max difference 0, in every run.
- Presents: p50 16.66–16.68 ms in every run (one per refresh, about 900 per 15 s), p95 17.08–22.16 ms,
  max 27.56 ms; no interval reached two refreshes (33.3 ms). Presents over 25 ms: 0 in 21 runs,
  1 in 4, 15 in one (hap1080-hap pass 2), with no skip.
- Against the simulated cadence above (pace 2 on the 120 Hz display: 4 of 26 runs of passes 1–2
  skipped 1–3 frames): there the clock was asked for a frame every second *present*, so two late
  presents in a row could leave a 33 ms gap (the mechanism of §1; those pass-1 runs have no
  per-refresh log, so their skips are not matched to single presents); on the real 60 Hz display the clock is asked at every refresh,
  and the presents came steadily (p50 16.67 ms), so the bursts measured at 120 Hz did not occur here.

## 2. A/V offset

**Method** (from `docs/handoffs/c2-time-and-sound.md`, DIAGNOSE 2). The clock picks the frame for the
moment the picture reaches the screen: one refresh after it is drawn (FIFO presentation, so the frame
drawn now is shown at the next vertical blank; up to one more refresh while a second frame is queued,
`desired_maximum_frame_latency: 2`). At that moment it asks the audio clock which movie time is heard:
the frame the device callback started with, at the time it reaches the speaker — cpal's `playback`
instant minus the callback's buffer, that is CoreAudio's host time of the buffer (when the device
consumes its first frame) plus the device latency and the safety offset — advanced by the stream
clock. The offset per refresh is *movie time heard − start of the frame shown*. A negative value: the
picture is ahead of the sound.

This is measured with the system's own clocks, **not** with a camera and a microphone. By the rule
(the nearest frame) the values lie within half a frame; their spread is the frame grid sampled at
the display's phase. The absolute offset rests on two estimates, each with a known error:
- sound: the safety offset is added although it lies before the host time (the sound is heard a
  few frames *earlier* than computed), and the stream latency, which cpal does not read, is missing
  (heard *later*). On this device cpal's whole reported delay is 13.97 ms = 512-frame buffer
  (11.61 ms) + 2.36 ms of device latency and safety offset;
- picture: when a second frame is queued the picture is shown one refresh (8.3 ms) *later* than
  estimated. With the bursty presents of section 1 this happens.

Swift Glance takes neither into account: its position is the player node's *render* time (ahead of
the heard sound by the output latency) and its frame is drawn one display-link tick and a main-thread
hop later; the two partly cancel. Its offset was never measured.

Results (pass 3, after the seam fix; `docs/data/c2-runs/pass3-audio/`), offset in ms:

| Clip | Sound → device | Cadence | Samples | Mean | p05 | p50 | p95 | Min | Max |
|---|---|---|---|---|---|---|---|---|---|
| `std_film_h264_audio.mov` (DXV, 25 fps) | **PCM** 48 kHz 2 ch → 44.1 kHz (resampled) | 120 Hz | 1736 | −0.20 | −17.9 | −0.3 | 18.0 | −19.9 | 20.0 |
| same | | 60 Hz cadence | 897 | 0.92 | −16.9 | 1.3 | 19.7 | −20.0 | 20.0 |
| `glenc_hapm_LEAKTEST_4k.mov` (HAP Q Alpha 2160p, 30 fps) | **PCM** 44.1 kHz 1 ch → 44.1 kHz | 120 Hz | 1735 | 1.59 | −10.5 | −0.1 | 15.0 | −14.6 | 15.3 |
| same | | 60 Hz cadence | 890 | 2.71 | −11.5 | 4.9 | 13.7 | −11.8 | 13.9 |
| `good-audio.mov` (DXV, 25 fps) | **AAC** 48 kHz 2 ch → 44.1 kHz (resampled) | 120 Hz | 1763 | −0.07 | −17.9 | 0.4 | 17.3 | −19.9 | 19.1 |
| same | | 60 Hz cadence | 898 | −0.16 | −18.7 | −0.4 | 17.8 | −19.0 | 19.7 |
| `test-hap1-audio.mov` (HAP 512x288, 30 fps) | **AAC** 44.1 kHz 1 ch → 44.1 kHz | 120 Hz | 1761 | −2.33 | −15.3 | −5.4 | 11.0 | −16.3 | 11.4 |
| same | | 60 Hz cadence | 882 | −0.68 | −16.2 | −0.1 | 16.4 | −16.6 | 16.6 |

Every sample lies within half a frame (20 ms at 25 fps, 16.7 ms at 30 fps); the means lie between
−2.3 and +2.7 ms and follow the display's phase against the frame grid, PCM and AAC alike.

## 3. Drift across 20 loops

The time between two wraps onto frame 0 (`loop_period_ms` in the JSON), each wrap stamped at the
refresh that showed frame 0 (so each stamp is quantised to the refresh, and late presents move a
stamp by up to 25 ms; the error does not add up across loops).

| Run | Clock | Loops | Mean period (clip length) | Over all loops |
|---|---|---|---|---|
| `glenc_hap5_alpha_real_chunks4.mov` (2160p HAP Alpha, 60 frames) | wall | 20 | 2000.189 ms (2000) | +3.8 ms |
| `glenc_hapm_LEAKTEST_4k.mov` (2160p HAP Q Alpha, 90 frames) | wall | 20 | 2999.561 ms (3000) | −8.8 ms |
| same | audio (PCM, 3.000 s) | 19 | 3000.428 ms (3000) | +8.1 ms |
| `good-audio.mov` (1080p DXV, 125 frames) | audio (AAC, 5.000 s) | 19 | 4999.744 ms (5000) | −4.9 ms |

Raw: `docs/data/c2-runs/c2-drift-wall-*.json`, `pass3-audio/c2-drift-audio-*.json` (+ CSV for the
PCM run). Every total is within the stamping resolution: **no measurable drift** on either clock,
0 frames skipped in all four runs (the wall clock wraps exactly by construction: DXVKit Plus's
transport integrates exact clock deltas and takes the position modulo the clip length; the clock
tests prove 0 wrong frames over 20 loops at 120 Hz, `the_wall_clock_does_not_drift_across_twenty_loops`).

**Found and fixed on the way: the loop seam on the audio clock.** The first 62 s audio-clock run
(`docs/data/c2-runs/c2-drift-audio-hapqa2160-3s.json`, before the fix) skipped one frame: frame 0.
Swift's rule (nearest frame, clamped to the clip) holds the last frame through the half frame before
the sound wraps, so the last frame showed for 1.5 frame periods (6 refreshes) and frame 0 for 0.5 (2
refreshes); one 24.7 ms present there skipped frame 0. The clock now shows the next loop's frame 0
in that half frame when the sound's loop is no longer than the picture (a longer sound still holds
the last frame, as Swift does) — **fixed, differs from Swift on purpose** (Amrit's rule for Swift
quirks: "fix"); test `at_the_loop_seam_the_nearest_frame_is_the_next_loops_frame_0`. After it: 0 skipped
in the 62 s and 101 s runs and all pass-3 runs, and no offset beyond half a frame (before it the seam
gave offsets up to +39.8 ms: `c2-drift-audio-dxv1080-aac-5s.json`; +32.7 ms on the HAP Q Alpha clip).

## 4. Seek, step, scrub; gain and mute

- **Frame n is frame n on every path** (`crates/glance-playback/tests/frames.rs`): the player's frame
  after shuffled seeks equals the sequential decode on 40 clips, 427 frames (every committed DXV and
  HAP fixture; H.264, HEVC, ProRes 4444, Motion JPEG through the OS decoder, read back from the GPU).
- **Swift Glance's eight frame-timing fixtures** (`tests/fixtures/frame-timing`, copied from Glance
  a9dc601; frame N has luma 5 N; B-frames at 23.976 / 25 / 29.97 / 30 / 60 fps, jittered H.264 and
  ProRes 4444): 40 of 40 by seek on each; the clock on the wall clock shows N from 2 µs after N's
  real start and N − 1 2 µs before it, 39 of 39 on each; the loop wraps onto frame 0 exactly.
  Negative control: the rate grid alone misses 4 starts on each of the three jittered clips.
- **Step, restart, scrub, end of clip, fallback, gain** — the clock's rules against a scripted sound
  (`crates/glance-playback/src/lib.rs` tests): step pauses (the sound too), moves one frame, sends that
  frame's start to the sound, clamps at both ends; restart goes to frame 0 and keeps play / pause;
  scrub pauses, shows each requested frame, resumes on release only if it was playing; at the end of a
  clip that does not loop the clock stops on the last frame and Play restarts from frame 0 on the wall
  clock and on the audio clock (**fixed on purpose**, Amrit: "fix"); with the sound silent for more than
  0.25 s the wall clock takes over at the frame on screen (held up to 0.249 s, wall from 0.26 s, no jump),
  and a sound that cannot start or fails falls back at once; volume and mute never move the frame,
  never reposition the sound and never leave the audio clock.
- **Scrub never builds a backlog** (`glance-proof scrub`, a new random frame every 8.33 ms for 10 s,
  1,200 requests per clip; `docs/data/c2-runs/c2-scrub-*.json`): **every request was answered with the
  latest frame asked for (0 of 1,200 otherwise, all five clips)**, and the time from request to frame
  equals a decode of the same frame from a second source at the same moment:

| Clip | Request → frame p50 / p95 / p99 / max (ms) | Same-moment decode p50 / p95 / p99 / max (ms) |
|---|---|---|
| `dxv_ycg6_3840x2160.mov` (DXV HQ 2160p, made) | 4.32 / 4.91 / 5.01 / 5.10 | 4.32 / 4.87 / 4.99 / 5.18 |
| `YCG6-DXV3.mov` (DXV HQ 1080p) | 1.08 / 2.57 / 3.04 / 6.97 | 1.05 / 2.58 / 2.95 / 3.25 |
| `Metric_01_HapM.mov` (HAP Q Alpha 1080p) | 1.34 / 2.65 / 3.21 / 3.84 | 1.50 / 2.65 / 3.04 / 3.72 |
| `glenc_hap5_alpha_real_chunks4.mov` (HAP Alpha 2160p) | 0.37 / 0.52 / 0.86 / 1.21 | 0.35 / 0.53 / 1.12 / 1.87 |
| `glenc_hapm_LEAKTEST_4k.mov` (HAP Q Alpha 2160p) | 1.93 / 3.16 / 3.62 / 4.61 | 2.17 / 3.23 / 3.70 / 5.35 |

## 5. Sound paths

- **An undecodable track plays the picture and says why**: `bad-audio-fixtures/bad-audio2.mp4`
  (H.264 with FLAC) → `failed (audio unreadable: unsupported: FLAC audio is not decoded)`, picture on
  the wall clock, 0 skipped (`docs/data/c2-runs/c2-fallback-flac.json`). FLAC stays unsupported, as in
  AVFoundation (open with Amrit, not blocking).
- **Decode**: every PCM layout DXVKit Plus reads (26 tracks of `audio_pcm_formats.mov`) and AAC with
  priming (`audio_aac_priming.mov`) decode through glance-audio; the non-interleaved `lpcm` track is
  refused with a reason (`crates/glance-audio/tests/decode.rs`).
- **Malformed formats** (0 or 65 channels, rate 0, −1, NaN, ∞) are refused with Swift's words
  (`malformed_formats_are_rejected_with_swifts_words`).
- **Resampling**: rubato's FFT resampler, once per clip at open; 44.1 → 48 kHz keeps the length
  (48,001 of 48,000 frames) and the tone's time within one output frame (measured 0.54 frames, 11 µs:
  rubato trims its delay in whole frames). The 1080p DXV runs played 48 kHz tracks on the 44.1 kHz
  device.
- The wrapped-rate GlEnc file (`in32` v1 stating 30,464 Hz over a 96,000 timescale) reads at 96 kHz
  through DXVKit Plus (Cowork decision: the correct speed; Swift plays it slow).

## 6. The 2160p DXV clips (no such clip in the test folder)

Made in the session's scratch folder with DXVKit 1.0.0's own `dxvtool` (Amrit's MIT encoder; built out
of tree from `.ref/dxvkit`, nothing written into that repo): 150 frames of `YCG6-DXV3.mov` (real
footage) decoded with `dxvtool dump-rgba`, upscaled 2x (pixel doubling), encoded with
`dxvtool encode - <out> --size 3840x2160 --variant ycg6|dxt1 --effort fast --fps 30`: YCG6 189 MB
(mean packet 1.26 MB), DXT1 96.5 MB. Not committed (test media stays out of the repo).

## 7. Intel build

`.cargo/config.toml` sets `MACOSX_DEPLOYMENT_TARGET = 14.0`. The x86_64 `glance-proof` starts under
Rosetta 2 (minos 14.0, exit 0) on this Mac and on the CI runner (run 43723686918); the same binary
built for rustc's default 10.12 fails in dyld (`Library not loaded: @rpath/libswiftCoreMedia.dylib`,
exit 134): `docs/data/c2-rosetta-load.txt`.
