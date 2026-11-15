# glance-core CHECKPOINT

Wave: c2-time-and-sound (closed, tag v0.2.0). Brief: ~/claude-handoffs/glance-core_2026-11-15_c2-time-and-sound_brief.md. Machine: <host> (Apple M5 Max, macOS 27.0.1). Previous wave: c1-exact-picture (closed, tag v0.1.0; its checkpoint is in git history at a0c2d04).

## DONE
- Read in full: the brief; the DXVKit Plus W2 / W3 / W4 notes and the Cowork W0 review note; roadmap (C2 row, crates); parity Core "Time", Glance "Transport" / "Audio"; Swift Glance a9dc601 `AudioClock.swift`, `FrameClock.swift`, `DXVPlayer.swift`, `HAPPlayer.swift`, `AVPlaybackController.swift`, `PlayerModel.swift` (transport); DXVKit Plus v0.11.0 `audio.rs`, `timing.rs`, `transport.rs`, `names.rs`, W4 handoff DIAGNOSE 1; cpal 0.18.2 CoreAudio output path (`src/host/coreaudio/macos/device.rs`); rubato 5.1.0 README and `Fft`. `.ref/DXVKit-Plus` moved to v0.11.0 (29fa9f5).
- IMPLEMENT 1: `dxvkit-plus-{core,wgpu,apple}` pinned to `v0.11.0` (29fa9f5a). Deleted: route.rs's FourCC walker, `display_name`, `unsupported_message`, `matroska_codec` (now `dxvkit_plus_core::{container::first_video_fourcc, names}`); `codec_label` (now `MediaInfo::codec_name`); the plane-based `dxv_variant_label` input (now `MediaInfo::dxv_variant`, label text kept); `open_gpu`'s limits code (now `Gpu::with_adapter`, DX12 choice kept) and `check_size` (DXVKit Plus checks). Flipped: two-track movie plays DXV; `KNOWN` gone; Amrit's files 6 refused, not 7; the message case uses damaged tables (+ negative control); size test makes the small device by hand; reserved BC7 is [0,0,0,0] on the exact path on every GPU. `GLANCE_REQUIRE_GPU=1 cargo test --workspace --release`: 31 passed, 0 failed.
- IMPLEMENT 2: `.cargo/config.toml` (`MACOSX_DEPLOYMENT_TARGET = 14.0`, `IPHONEOS_DEPLOYMENT_TARGET = 15.0`). Intel glance-proof loads under Rosetta (minos 14.0); negative control built for 10.12 fails in dyld, exit 134 (`docs/data/c2-rosetta-load.txt`). CI builds and starts it under Rosetta.

- IMPLEMENT 3: `glance-audio` (cpal 0.18.2, rubato 5.1.0 `Fft`): first audio track decoded whole (DXVKit Plus `AudioReader`), Swift's format check, resampled once to the device's own rate, channel map in the callback, gain / mute as an atomic gain, heard-time position from CoreAudio timestamps (cpal's playback instant minus the buffer), wrap / hold like Swift. `glance_media::open_audio` (backend choice stays in glance-media). Tests: 5 unit + 2 on CC0 fixtures copied from DXVKit Plus v0.11.0 (`audio_pcm_formats.mov`, `audio_aac_priming.mov`).
- IMPLEMENT 4: `glance-playback::Clock` (audio master: nearest frame at the display instant, clamped; 0.25 s fallback re-anchored; wall clock on DXVKit Plus's transport; seek / step / restart / scrub / loop; end of a non-looping clip: Play restarts from 0, audio and wall paths) behind an `AudioMaster` trait; `ClipPlayer` with `Sound::{Device, Off}`; `Clip::from_timing` everywhere (`clip_of`). Tests: 7 clock-rule tests with a scripted sound; `tests/frames.rs`: shuffled seeks = sequential decode on 40 clips / 427 frames (DXV, HAP, native via GPU read-back); Swift Glance's 8 frame-timing fixtures (copied, MIT) 40 of 40 by seek, every frame start by the clock, loop onto 0; negative control: the grid alone misses 4 starts on each jittered clip.
- IMPLEMENT 5: glance-proof plays with sound (`--mute`, `--no-audio`, `--pace N`, `--no-loop`), reports master, audio state, device / track format, cpal delay, A/V samples, loop periods; `scrub` and `media` subcommands. Smoke run (Arena running, not a measurement): 2 DXV clips with PCM / AAC on the audio clock, 0 skipped at 120 Hz, device 44.1 kHz (48 kHz tracks resampled), cpal delay 13.97 ms (512-frame buffer).
- 2160p DXV test clips made in the scratchpad (none in Amrit's folder): DXVKit 1.0.0 `dxvtool` (built out of tree) encoding 150 frames of `YCG6-DXV3.mov` decoded and upscaled 2x, YCG6 and DXT1.
- Gates: fmt, clippy `-D warnings`, `GLANCE_REQUIRE_GPU=1 cargo test --workspace --release` 47 passed 0 failed at that point (49 on the final tree), `cargo deny check` ok.

- Amrit 2026-11-15: quit Arena ("I've quit Arena, go"); 60 Hz simulated ("Simulate at 120 Hz"); Blender stays open ("Keep Blender open").
- Measurements (`docs/05-C2-RESULTS.md`, raw `docs/data/c2-runs/`): 120 Hz 0 skipped in 30 runs (1080p / 2160p HAP and DXV, with and without sound); 60 Hz cadence 26 of 30 runs 0, 4 first-pass runs skipped 1–3 (30 fps clips; bursty presents, p95 16.7 ms in 65 of 67 runs, the v0.1.0 build alike); A/V samples within half a frame on PCM and AAC (means −2.3 … +2.7 ms); drift over 19–20 loops within stamping resolution, both clocks; scrub 6,000 of 6,000 latest, request = same-moment decode; FLAC → picture on the wall clock with the reason.
- Found by measuring and fixed: the audio loop seam (Swift's clamp gave frame 0 half a frame; skipped once in 20 loops) → `seam_wraps`, test `at_the_loop_seam_the_nearest_frame_is_the_next_loops_frame_0`; all audio runs re-measured after it (pass 3): 0 skipped. `--no-audio` now reads "sound off". Test `a_one_frame_clip_shows_its_frame`.
- Docs: `docs/05-C2-RESULTS.md`; parity (Core Time 4 of 6, Glance Audio 5 of 8 + 2 core halves, Transport 4 + 5 core halves); roadmap, README, CLAUDE.md layout; handoff draft `docs/handoffs/c2-time-and-sound.md`. Workspace version 0.2.0.

- Verifier: 62 confirmed, 5 wrong (all corrected), 8 unproven (2 settled by committing `docs/data/c2-runs/diagnose/`, 6 stated in the handoff). CI green on 6ed73de (run 43728013074).
- Handoff `docs/handoffs/c2-time-and-sound.md`, copied to `~/claude-handoffs/glance-core_2026-11-15_c2-time-and-sound-result.md`. Tag `v0.2.0`.
- **Wave closed.**

## NEXT
- Nothing queued in this repo. Next core wave per the roadmap: C3 (shell), when Amrit writes its brief.
- Deferred (not C2's brief; reasons in the handoff): `alpha_rule` from `MediaInfo::alpha`; DXVKit Plus's Apple island under `cfg(ios)`; reserved-BC7 HAP R generation; re-resampling when the output device changes rate while playing; a real 60 Hz display run (Amrit's call).

## STATE
- Test media (read-only): `<testfiles>` (174 movie files).
- Pins: DXVKit Plus `v0.11.0` (29fa9f5); wgpu 30.0.x, egui 0.36.2; rustc 1.99.1 (Homebrew, the build), 1.100.0 (rustup stable, CI and the Intel build: Homebrew's rustc has no x86_64 std).
- 2160p DXV clips (not in the repo): scratchpad `synth/dxv_{ycg6,dxt1}_3840x2160.mov`, recipe in `docs/05-C2-RESULTS.md` §6. Measurement runner: `tools/c2-measure.sh <scratch> <out>`.
- Gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --release -- -D warnings`, `cargo test --workspace --release`, `cargo deny check`, `cargo build --release`.

## OPEN

## IDEAS
- Present timestamps from the OS (CAMetalLayer presented time) for the A/V estimate instead of "one refresh after drawing".
- A camera + microphone A/V check (flash and click clip) to measure the absolute offset.
- Stream the audio instead of decoding the whole track for very long clips.
