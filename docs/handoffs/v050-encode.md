# v0.5.0 — glance-encode, made from GlEnc's goal run (2026-11-17)

Brief: `~/claude-handoffs/glenc-rs_2026-11-17_goal-build-to-swap_brief.md` (HOW TO WORK 4: core needs are made here).

## What shipped
- **`glance-encode`** (new, `#![forbid(unsafe_code)]`): Swift GlEnc 1.6.2's `GlEncCore` encode path — `encode(&EncodeRequest,
  progress, cancel)`: the source routed by its first video track's FourCC (DXV `DXT1 DXT5 YCG6 YG10 DXDI DXD3 DXD4`, HAP
  `Hap1 Hap5 HapY HapM`, else the platform), Swift's source limits and words, the DXV rate (frames / mdhd duration) and HAP
  rate (frames / summed deltas) of Glance 0.18.1's demuxers, the native rate by FrameTiming and frame count
  round(duration × fps), CoreMedia's `CMTime(seconds:, 600)` (it truncates: measured), crop → resize (Nearest; vImage
  Bilinear / Lanczos; letterbox on opaque black) → encode → write, the trim window and its progress, cancel before every
  frame, the partial file removed on any failure; alpha: DXV Alpha Type un-premultiplied on read, a native format stated
  premultiplied un-premultiplied when the output keeps alpha, straight in every writer; the decoder's buffer
  attachments kept only for frames passed through untouched (they give AVAssetWriter's `colr`); audio: s32le at the
  chosen rate capped by the container, re-read at the cap, a warning instead of a failure; trimmed jobs carry the audio of
  their frames only (Amrit 2026-11-14, "fix"); HAP's refined BC4 switch.
- Shell: `ShellEvent::DroppedAll(Vec<PathBuf>)`, `dialog::open_movies`, `glance_platform::reveal_in_finder` (macOS).
- DXVKit Plus pinned at **v0.12.0** (4343fe9).

## Evidence
- Unit 9 + integration 8 (`crates/glance-encode/tests/encode.rs`, CC0 fixtures): every DXV / HAP format from DXV, HAP and
  native sources; trim / crop / resize; Swift's error words; cancel and failure leave no file; progress over the trim.
- Byte identity with Swift GlEnc 1.6.2 is measured in glenc-rs (its oracle): 1,543 / 1,543 DXV / HAP outputs from every
  non-HAP-Q source identical (`glenc-rs docs/data/e3/xtest-dxv-hap.jsonl`); platform outputs equal in container, streams
  and pictures (`platform-deep-compare.jsonl`).
- Parity: core item "Writers write straight alpha" ticked.

## Gates
fmt ok · clippy --workspace --all-targets --release -D warnings ok · `GLANCE_REQUIRE_GPU=1 cargo test --workspace
--release`: 89 passed, 0 failed · cargo deny ok · cargo build --release ok · CI: see CHECKPOINT.
