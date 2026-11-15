# glance-core CHECKPOINT

Wave: c2-time-and-sound (open). Brief: ~/claude-handoffs/glance-core_2026-11-15_c2-time-and-sound_brief.md. Machine: <host> (Apple M5 Max, macOS 27.0.1). Previous wave: c1-exact-picture (closed, tag v0.1.0; its checkpoint is in git history at a0c2d04).

## DONE
- Read in full: the brief; the DXVKit Plus W2 / W3 / W4 notes and the Cowork W0 review note; roadmap (C2 row, crates); parity Core "Time", Glance "Transport" / "Audio"; Swift Glance a9dc601 `AudioClock.swift`, `FrameClock.swift`, `DXVPlayer.swift`, `HAPPlayer.swift`, `AVPlaybackController.swift`, `PlayerModel.swift` (transport); DXVKit Plus v0.11.0 `audio.rs`, `timing.rs`, `transport.rs`, `names.rs`, W4 handoff DIAGNOSE 1; cpal 0.18.2 CoreAudio output path (`src/host/coreaudio/macos/device.rs`); rubato 5.1.0 README and `Fft`. `.ref/DXVKit-Plus` moved to v0.11.0 (29fa9f5).
- IMPLEMENT 1: `dxvkit-plus-{core,wgpu,apple}` pinned to `v0.11.0` (29fa9f5a). Deleted: route.rs's FourCC walker, `display_name`, `unsupported_message`, `matroska_codec` (now `dxvkit_plus_core::{container::first_video_fourcc, names}`); `codec_label` (now `MediaInfo::codec_name`); the plane-based `dxv_variant_label` input (now `MediaInfo::dxv_variant`, label text kept); `open_gpu`'s limits code (now `Gpu::with_adapter`, DX12 choice kept) and `check_size` (DXVKit Plus checks). Flipped: two-track movie plays DXV; `KNOWN` gone; Amrit's files 6 refused, not 7; the message case uses damaged tables (+ negative control); size test makes the small device by hand; reserved BC7 is [0,0,0,0] on the exact path on every GPU. `GLANCE_REQUIRE_GPU=1 cargo test --workspace --release`: 31 passed, 0 failed.
- IMPLEMENT 2: `.cargo/config.toml` (`MACOSX_DEPLOYMENT_TARGET = 14.0`, `IPHONEOS_DEPLOYMENT_TARGET = 15.0`). Intel glance-proof loads under Rosetta (minos 14.0); negative control built for 10.12 fails in dyld, exit 134 (`docs/data/c2-rosetta-load.txt`). CI builds and starts it under Rosetta.

- IMPLEMENT 3: `glance-audio` (cpal 0.18.2, rubato 5.1.0 `Fft`): first audio track decoded whole (DXVKit Plus `AudioReader`), Swift's format check, resampled once to the device's own rate, channel map in the callback, gain / mute as an atomic gain, heard-time position from CoreAudio timestamps (cpal's playback instant minus the buffer), wrap / hold like Swift. `glance_media::open_audio` (backend choice stays in glance-media). Tests: 5 unit + 2 on CC0 fixtures copied from DXVKit Plus v0.11.0 (`audio_pcm_formats.mov`, `audio_aac_priming.mov`).
- IMPLEMENT 4: `glance-playback::Clock` (audio master: nearest frame at the display instant, clamped; 0.25 s fallback re-anchored; wall clock on DXVKit Plus's transport; seek / step / restart / scrub / loop; end of a non-looping clip: Play restarts from 0, audio and wall paths) behind an `AudioMaster` trait; `ClipPlayer` with `Sound::{Device, Off}`; `Clip::from_timing` everywhere (`clip_of`). Tests: 7 clock-rule tests with a scripted sound; `tests/frames.rs`: shuffled seeks = sequential decode on 40 clips / 427 frames (DXV, HAP, native via GPU read-back); Swift Glance's 8 frame-timing fixtures (copied, MIT) 40 of 40 by seek, every frame start by the clock, loop onto 0; negative control: the grid alone misses 4 starts on each jittered clip.
- IMPLEMENT 5: glance-proof plays with sound (`--mute`, `--no-audio`, `--pace N`, `--no-loop`), reports master, audio state, device / track format, cpal delay, A/V samples, loop periods; `scrub` and `media` subcommands. Smoke run (Arena running, not a measurement): 2 DXV clips with PCM / AAC on the audio clock, 0 skipped at 120 Hz, device 44.1 kHz (48 kHz tracks resampled), cpal delay 13.97 ms (512-frame buffer).
- 2160p DXV test clips made in the scratchpad (none in Amrit's folder): DXVKit 1.0.0 `dxvtool` (built out of tree) encoding 150 frames of `YCG6-DXV3.mov` decoded and upscaled 2x, YCG6 and DXT1.
- Gates: fmt, clippy `-D warnings`, `GLANCE_REQUIRE_GPU=1 cargo test --workspace --release` 47 passed 0 failed, `cargo deny check` ok.

## NEXT
- Measurements (no-skip 60 / 120 Hz, A/V offset, drift, seek / step / scrub) → `docs/05-C2-RESULTS.md`; parity; handoff; tag v0.2.0.

## STATE
- Test media (read-only): `<testfiles>` (174 movie files).
- Pins: DXVKit Plus `v0.11.0` (29fa9f5); wgpu 30.0.x, egui 0.36.2; rustc 1.99.1 (Homebrew, the build), 1.100.0 (rustup stable, CI and the Intel build: Homebrew's rustc has no x86_64 std).
- Gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --release -- -D warnings`, `cargo test --workspace --release`, `cargo deny check`, `cargo build --release`.

## OPEN
- Measurements need an idle machine: Arena was running at 15:2x (Amrit's). A true 60 Hz run needs the display set to 60 Hz (ask first).
- Not done on purpose (outside C2's brief): `alpha_rule` reading `MediaInfo::alpha` for native clips (W2: "can"), the Apple island under `cfg(ios)`, generated HAP R movies with reserved blocks.
