# glance-core

The shared Rust core of the Glance family (Glance, CueGlance, GlEnc, MaCrate, later GlancEhr):
egui + wgpu, macOS and Windows, on top of [DXVKit Plus](https://github.com/AmritusG/DXVKit-Plus).

Start with `CLAUDE.md`, then `docs/00-ROADMAP.md`. State of the work: `CHECKPOINT.md`.

```sh
cargo build --release
./target/release/glance-proof clip_hap.mov clip_dxv.mov        # two clips in a window, timing logged
./target/release/glance-proof verify clip.mov a-folder/        # the render harness, no window
```

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
