# glance-core

The shared Rust core of the Glance family (Glance, CueGlance, GlEnc, MaCrate, later GlancEhr):
egui + wgpu, macOS and Windows, on top of [DXVKit Plus](https://github.com/AmritusG/DXVKit-Plus).

Start with `CLAUDE.md`, then `docs/00-ROADMAP.md`. State of the work: `CHECKPOINT.md`.

```sh
cargo build --release
./target/release/glance-proof clip_hap.mov clip_dxv.mov        # clips in a window, with sound, timing logged
./target/release/glance-proof verify clip.mov a-folder/        # the render harness, no window
```

## What 0.3.0 adds (core wave C3, "the shell")

- **Settings in the Swift apps' own store** (`glance-settings` over `glance-platform`): the
  `UserDefaults` domain of the app's bundle id, Swift's keys, encodings and read rules — Swift
  Glance's own code and the Rust store read each other's writes.
- **The OS around the window** (`glance-platform`, the one crate with `unsafe`): files from Finder
  at cold launch and while running into one window, the "Set as Default" flow, media keys and Now
  Playing, recents; Windows single instance, settings folder and media controls written for C4.
- **Glance's look and shell in egui** (`glance-ui`): the system font, the transport bar and scrub
  row measured equal to Swift Glance's, the menus (muda) and dialogs (rfd) with Swift's items and
  words, the window-fit rule, `run_shell`.
- **A packaging kit** (`tools/package/`): universal build, Developer ID signing, notarisation,
  staple, DMG + `.sha256`, exactly one LaunchServices record — proven on `glance-sample`.
- The output device changing rate mid-clip rebuilds the sound at the new rate; native clips read
  their stated alpha; egui's bundled fonts are gone. Numbers: `docs/06-C3-RESULTS.md`.

## What 0.2.0 adds (core wave C2, "time and sound")

- **Sound and the clock.** `glance-audio` plays a clip's first audio track (PCM, and AAC through
  the OS) on the output device at its own rate; `glance-playback`'s clock follows the sound heard
  when the picture is shown (nearest frame), falls back to the wall clock after 0.25 s without it,
  and has Glance's transport rules (seek, step, restart, scrub, loop, end of clip). Frame n is frame n
  on every decode path. Numbers: `docs/05-C2-RESULTS.md`.
- DXVKit Plus pinned at 0.11.0; the workarounds it replaced are gone. Intel macOS binaries load
  (deployment target 14.0).

## What 0.1.0 holds (core wave C1, "exact picture")

- **The picture is exact.** A DXV or HAP frame shown at 1:1 equals the CPU decode, every texel,
  every channel, every variant, at every size from 1x1 to 3840x2160 — DXVKit Plus's exact block
  path, plus this core's Alpha Type pass in whole code values. Window-scaled it is within 1 of a
  software bilinear model. Numbers: `docs/04-C1-RESULTS.md`.
- **Routing by content**, with the Swift apps' wording for a file nothing plays
  (`glance_media::route`).
- **The Alpha Type** of a clip: the rule, the "looks premultiplied / looks straight" hint and the
  key the choice is remembered by, identical to Swift Glance's (`glance_media::alpha_report`,
  `clip_key`).
- A headless stage (`glance_ui::headless`) that draws the real egui widget offscreen, for tests.

## Gates

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --release -- -D warnings
cargo test --workspace --release
cargo deny check
cargo build --release
```

The GPU tests need a GPU (Metal on macOS); without one they skip, unless `GLANCE_REQUIRE_GPU=1`
makes that a failure. CI (`.github/workflows/ci.yml`) runs the same gates on every push.

## Licence

MIT (`LICENSE`), Copyright (c) 2026 Amrit Rosell. What came from elsewhere, and the test
fixtures' licences: `ATTRIBUTION.md`. No FFmpeg, libav, GStreamer or libmpv; dependencies are
permissive only (`deny.toml`).
