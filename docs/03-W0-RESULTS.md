# W0 results — the proof

2026-11-14. One HAP and one DXV clip playing side by side in an egui window on macOS, through the
new core crates, with frame timing logged. Release build only.

## What ran

```
file.mov ─ glance-media (probe / open; DXVKit Plus 0.7.1 demux + HAP / DXV decode)
         ─ glance-playback (DXVKit Plus read-ahead cache + exact-time transport on the host clock)
         ─ glance-gpu (BC block upload + as-stored RGBA by DXVKit Plus; the Alpha Type pass)
         ─ glance-ui (egui 0.36 on winit, wgpu 30 / Metal; checkerboard, picture blended over it)
```

Binary: `target/release/glance-proof <clip>... --seconds 15 --out … --log …` (1 s warm-up before
measuring, so the window appearing and the display ramping to full refresh are not counted).
Machine: Apple M5 Max, 128 GB, macOS 27.0.1, built-in display at 120 Hz, `PresentMode::Fifo`.
rustc 1.99.1.

## Test media (Amrit's own, read-only, not copied)

Folder: `<testfiles>` (the `<testfiles>`
the Swift repos name does not exist on this Mac; this is the copy that does).

| Run | HAP file | DXV file |
|---|---|---|
| **A (the gate)** | `Metric_01_HapM.mov` — HAP Q Alpha, 1920x1080, 25 fps, 201 frames | `DXT5-DXV3.mov` — DXV3 Normal Quality With Alpha, 1920x1080, 30 fps, 300 frames |
| B | `glenc_hap1_1920x1080_chunks4.mov` — HAP (4 chunks), 1920x1080, 30 fps | `hap-testclips/std_film_h264_audio_DXV High Quality With Alpha [00.00.00-00.10.00].mov` — DXV3 High Quality With Alpha, 1920x1080, 25 fps |
| C | `glenc_hapm_LEAKTEST_4k.mov` — HAP Q Alpha, 3840x2160, 30 fps | `DXT1-DXV.mov` — DXV3 Normal Quality, 1920x1080, 30 fps |
| D (extra: native path) | `_master_hapm_3840x2160.mov` — despite its name **ProRes 4444** (`ap4h`), 3840x2160, 30 fps, through AVFoundation / VideoToolbox, zero-copy | `YCG6-DXV3_DXV Normal Quality With Alpha.mov` — DXV3 Normal Quality With Alpha, 1920x1080, 30 fps |

## Numbers (15.0 s measured per run; raw: `docs/data/w0-proof-{a,b,c,d}.json`, per-refresh rows in the `.csv`)

Window (present to present, ms):

| Run | Presents | mean | p50 | p95 | p99 | max | Refreshes over 12 ms |
|---|---|---|---|---|---|---|---|
| A | 1797 | 8.350 | 8.34 | 8.50 | 8.58 | 17.00 | 4 |
| B | 1800 | 8.333 | 8.33 | 8.51 | 8.60 | 8.74 | 0 |
| C | 1800 | 8.333 | 8.34 | 8.51 | 8.59 | 8.68 | 0 |
| D | 1800 | 8.333 | 8.33 | 8.48 | 8.56 | 8.72 | 0 |

Per clip. *Shown* = times the frame on screen changed; *skipped* = file frames jumped over;
*fetch* = wait for the decoded frame on the UI thread; *show* = CPU time of upload + conversion +
Alpha Type pass (recorded and submitted; GPU time is not in it); *picture vs CPU* = the shown
picture read back from the GPU against DXVKit Plus's CPU decode of the same frame, in code values.

| Run | Clip | Shown | Skipped | Shown fps (clip fps) | fetch p50 / p99 / max ms | show p50 / p99 / max ms | Cache misses | Picture vs CPU max / mean |
|---|---|---|---|---|---|---|---|---|
| A | HAP Q Alpha 1080p | 376 | **0** | 25.06 (25) | 0.008 / 0.018 / 0.02 | 0.92 / 1.45 / 1.56 | 1 | 2 / 0.148 |
| A | DXV NQ Alpha 1080p | 451 | **0** | 30.06 (30) | 0.007 / 0.017 / 0.02 | 0.64 / 1.11 / 1.22 | 1 | 2 / 0.089 |
| B | HAP 1080p, 4 chunks | 451 | **0** | 30.07 (30) | 0.008 / 0.021 / 0.03 | 0.50 / 0.75 / 0.85 | 1 | 1 / 0.103 |
| B | DXV HQ Alpha 1080p | 376 | **0** | 25.07 (25) | 0.007 / 0.024 / 0.03 | 0.91 / 1.31 / 1.43 | 1 | **0 / 0** |
| C | HAP Q Alpha 2160p | 451 | **0** | 30.07 (30) | 0.008 / 0.017 / 0.02 | 2.60 / 3.65 / 4.50 | 1 | 1 / 0.049 |
| C | DXV NQ 1080p | 451 | **0** | 30.07 (30) | 0.006 / 0.015 / 0.02 | 0.35 / 0.60 / 0.71 | 1 | 1 / 0.076 |
| D | ProRes 4444 2160p (native) | 451 | **0** | 30.07 (30) | 0.005 / 0.010 / 0.01 | 0.33 / 1.68 / 2.15 | 1 | not checked (no CPU decode of native frames) |
| D | DXV NQ Alpha 1080p | 451 | **0** | 30.07 (30) | 0.006 / 0.022 / 0.03 | 0.59 / 1.08 / 1.21 | 1 | 1 / 0.207 |

The one cache miss per clip is its first frame (decoded on the spot); every other frame came from
read-ahead (8 frames ahead, 4 worker threads). Shown fps reads 0.05 above the clip rate because the
first showing is counted at time zero (376 showings = 375 frame periods in 15.0 s = 25.0).
No decode error, no worker error, exit code 0 in every run.

## What this proves, and what it does not

Proven, by the numbers above:
- A HAP and a DXV clip open and play through the new core in one egui window on wgpu / Metal:
  every file frame reached the screen in order (0 skipped) at the clip's own rate, on a 120 Hz
  display, 1080p and 2160p, with the UI thread never waiting more than 0.03 ms for a frame.
- The picture on the GPU is the decoded frame: within 2 / 255 of the CPU decode for the BC-sampled
  paths (HAP, HAP Q, DXV Normal Quality), exact for DXV High Quality. These are DXVKit Plus's own
  stated contracts.
- One wgpu device serves the decoder upload, the Alpha Type pass and egui (no copy between them),
  with DXVKit Plus 0.7.1, egui 0.36.2 and wgpu 30 resolving to a single wgpu.
- The native macOS path (ProRes 4444 through VideoToolbox, zero-copy) plays through the same crates.

Not proven here:
- **Nobody looked at the window.** Claude judges on numbers, not screenshots; the read-back check
  covers the picture texture, not egui's final draw of it. Amrit's eyes for ten seconds:
  `./target/release/glance-proof "<hap>.mov" "<dxv>.mov"` (no `--seconds`: it stays open; the
  Alpha Type menu and the Black / Checkerboard switch are live).
- **Glance's exactness bar is not met for the BC-sampled paths.** Glance renders every DXV and HAP
  variant at max difference 0 against the CPU decode by decoding the blocks in the shader from
  integer textures (Apple GPUs' fixed-function BC decode is off by up to 2). DXVKit Plus does that
  for DXV High Quality only; DXT1 / DXT5 and HAP go through the GPU's BC decoder (1–2 measured
  here). Closing it is DXVKit Plus work (brief written) and a parity item.
- Audio, the audio-master clock, scrubbing, seeking, speed, looping modes other than loop, HAP
  alpha-only / HAP R / HAP HDR / legacy DXV, Windows. All later waves.
- An earlier build of the proof (same decode path, before the Alpha Type pass was reworked)
  showed 1.6–4 % of refreshes taking two display periods (28–74 of ~1,800) while five recon agents
  were also running on the machine; the final build shows 0–4 per run with two agents running.
  The cause was not isolated (machine load or the changed draw path). No clip frame was skipped in
  either case. To be measured properly in the playback wave, on an idle machine.

## Gates

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets --release -- -D warnings`,
`cargo test --workspace --release` (10 tests: 4 media, 2 playback, 3 GPU present-pass tests on the
real GPU with a negative control each, 1 UI), `cargo build --release`: all pass. Receipts in the
handoff.
