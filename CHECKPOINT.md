# glance-core CHECKPOINT

Wave: c2-time-and-sound (open). Brief: ~/claude-handoffs/glance-core_2026-11-15_c2-time-and-sound_brief.md. Machine: <host> (Apple M5 Max, macOS 27.0.1). Previous wave: c1-exact-picture (closed, tag v0.1.0; its checkpoint is in git history at a0c2d04).

## DONE
- Read in full: the brief; the DXVKit Plus W2 / W3 / W4 notes and the Cowork W0 review note; roadmap (C2 row, crates); parity Core "Time", Glance "Transport" / "Audio"; Swift Glance a9dc601 `AudioClock.swift`, `FrameClock.swift`, `DXVPlayer.swift`, `HAPPlayer.swift`, `AVPlaybackController.swift`, `PlayerModel.swift` (transport); DXVKit Plus v0.11.0 `audio.rs`, `timing.rs`, `transport.rs`, `names.rs`, W4 handoff DIAGNOSE 1; cpal 0.18.2 CoreAudio output path (`src/host/coreaudio/macos/device.rs`); rubato 5.1.0 README and `Fft`. `.ref/DXVKit-Plus` moved to v0.11.0 (29fa9f5).
- IMPLEMENT 1: `dxvkit-plus-{core,wgpu,apple}` pinned to `v0.11.0` (29fa9f5a). Deleted: route.rs's FourCC walker, `display_name`, `unsupported_message`, `matroska_codec` (now `dxvkit_plus_core::{container::first_video_fourcc, names}`); `codec_label` (now `MediaInfo::codec_name`); the plane-based `dxv_variant_label` input (now `MediaInfo::dxv_variant`, label text kept); `open_gpu`'s limits code (now `Gpu::with_adapter`, DX12 choice kept) and `check_size` (DXVKit Plus checks). Flipped: two-track movie plays DXV; `KNOWN` gone; Amrit's files 6 refused, not 7; the message case uses damaged tables (+ negative control); size test makes the small device by hand; reserved BC7 is [0,0,0,0] on the exact path on every GPU. `GLANCE_REQUIRE_GPU=1 cargo test --workspace --release`: 31 passed, 0 failed.
- IMPLEMENT 2: `.cargo/config.toml` (`MACOSX_DEPLOYMENT_TARGET = 14.0`, `IPHONEOS_DEPLOYMENT_TARGET = 15.0`). Intel glance-proof loads under Rosetta (minos 14.0); negative control built for 10.12 fails in dyld, exit 134 (`docs/data/c2-rosetta-load.txt`). CI builds and starts it under Rosetta.

## NEXT
- glance-audio (cpal output, rubato resample, channel map, gain / mute, format checks, failure states).
- glance-playback: audio-master clock, 0.25 s fallback, transport rules; glance-proof with sound.
- Measurements (no-skip 60 / 120 Hz, A/V offset, drift, seek / step / scrub) → `docs/05-C2-RESULTS.md`; parity; handoff; tag v0.2.0.

## STATE
- Test media (read-only): `<testfiles>` (174 movie files).
- Pins: DXVKit Plus `v0.11.0` (29fa9f5); wgpu 30.0.x, egui 0.36.2; rustc 1.99.1 (Homebrew, the build), 1.100.0 (rustup stable, CI and the Intel build: Homebrew's rustc has no x86_64 std).
- Gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --release -- -D warnings`, `cargo test --workspace --release`, `cargo deny check`, `cargo build --release`.

## OPEN
- Not done on purpose (outside C2's brief): `alpha_rule` reading `MediaInfo::alpha` for native clips (W2: "can"), the Apple island under `cfg(ios)`, generated HAP R movies with reserved blocks.
