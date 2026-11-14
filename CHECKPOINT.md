# glance-core CHECKPOINT

Wave: c1-exact-picture. Brief: ~/claude-handoffs/glance-core_2026-11-14_c1-exact-picture_brief.md. Machine: <host> (Apple M5 Max, macOS 27.0.1).

## DONE
- Read in full: the brief, the two notes it names, roadmap, parity (Core + Glance routing / exactness / alpha), research §0 §3 §9. `.ref/DXVKit-Plus` refreshed to `v0.8.0` (9bc1977).
- Diagnosis in source (Swift Glance a9dc601, DXVKit v1.0.0, DXVKit Plus v0.8.0), a source per fact: `docs/handoffs/c1-exact-picture.md` (DIAGNOSE 1–6).
- Re-pin: `dxvkit-plus-{core,wgpu,apple}` tag `v0.8.0` (Cargo.lock 9bc1977c). No HAP Q hash was pinned in this repo from 0.7.1.
- `glance-gpu`: `ClipView` on `BlockPath::Exact` by default, `with_path` / `set_path` / `path`; the present pass in whole code values (as stored / straight = round(c·a/255) / alpha-only = white at that alpha); `expected_picture`; `verify` (the harness); `open_gpu` (the adapter's texture limit, 16,384 here) and a size check that returns an error value.
- `glance-media`: `route` + `NotPlayable` (Swift Glance's messages), `Clip::open`, `first_video_fourcc`, `display_name`, `clip_key`, `alpha_report`. `ClipPlayer::open` routes through it.
- `glance-ui`: `headless::Stage` (the real widget offscreen), `on_screen` (the bilinear model).
- `glance-proof`: `verify`, `hapq`, `gpu` subcommands.
- Fixtures from DXVKit Plus v0.8.0 with `cp -p` (CC0 tile movies, synthetic HAP R / HDR, DXVKit's MIT vectors): `tests/fixtures/`, 1.2 MB. `ATTRIBUTION.md`.
- Tests: `cargo test --workspace --release` 32 passed, 0 failed (also with `GLANCE_REQUIRE_GPU=1`). 1:1: 180 rows (12 variants x 9 sizes x Alpha Types) max 0; 36 fixture movies, 563 frame comparisons, max 0; 64 DXVKit vector pictures hash to DXVKit's RGBA. Scaled: 134 screens, largest distance 1, none beyond. Hostile files: 2,930 cases, no crash, no hang.
- Swift cross-test (Swift Glance's own GlanceCore run on the same files; output in `tests/data/swift-glance-*.tsv`): route, message, FourCC, clip key and hint equal on 52 fixtures and on Amrit's 174 files except one route (`adv_trunc_hapm_midframe.mov`, pinned as known).
- HAP Q against Swift's convention, every frame of Amrit's 31 HapY / HapM files: max 2, 43.42 % of texels (43.26 % by 1, 0.16 % by 2). `docs/data/c1-hapq-testfiles.json`.
- Amrit's decisions applied: licence MIT (`LICENSE`, workspace `license = "MIT"`, version 0.1.0), `README.md`, `docs/02-PARITY.md` (28 edits; Core "Decode and render" and "Alpha and background" ticked with evidence, what is not tickable on Metal said), `docs/00-ROADMAP.md` (25 edits), `docs/01-RESEARCH.md` §9 (16 decisions closed).
- `cargo deny check`: advisories, bans, licenses, sources ok (`deny.toml`; two crate-scoped exceptions, one needs Amrit's yes: egui's built-in fonts).
- CI: `.github/workflows/ci.yml`. First run (6b80bf1): lint ok, Windows check ok, macOS: the runner has a Metal GPU ("Apple Paravirtual device"); 176 of 180 sweep rows max 0 there, the 4 failing rows were HAP R with reserved-mode BC7 blocks (the virtual GPU decodes those differently). Generated BC7 blocks are now valid ones; reserved blocks have their own test.
- Cross targets: `cargo check --workspace` ok for `x86_64-apple-darwin` and `aarch64-apple-ios` (rustc 1.100, local); `x86_64-pc-windows-msvc` ok on the Windows runner.

- Amrit's clips through the harness, every frame (`glance-proof verify`, 667 s): 174 files — 134 DXV / HAP clips, 225 rows, 43,475 frame comparisons, 65,222,310,280 texels, **max 0**, 0 differing; scaled 450 screens, largest distance 1, none beyond; 33 OS-codec clips shown (11,378 frames, none blank); 7 refused with the message. `docs/data/c1-verify-testfiles.json`. Found and fixed on the way: a 3840x9626 clip above wgpu's default texture limit.
- Proof window smoke run on the exact path (5 s, the W0 gate pair, harness running on the same GPU): 0 skipped, picture vs CPU max 0. `docs/data/c1-window-smoke.json`.
- `docs/04-C1-RESULTS.md` written from the raw data.
- CI green three times on main (2069c2b run 43638144973, ba20a42 run 43638498291); the runner's GPU returns opaque black for a reserved BC7 block (recorded by a CI step).
- Briefs for DXVKit Plus: `~/claude-handoffs/DXVKit-Plus_2026-11-14_c1-findings_brief-r2.md` (eight findings; supersedes the first, which lacked the BC7 one).
- Gates on the final tree: fmt ok; clippy `-D warnings` ok (rustc 1.99.1 and 1.100.0); `GLANCE_REQUIRE_GPU=1 cargo test --workspace --release` 32 passed, 0 failed; `cargo deny check` ok; `cargo build --release` ok.

## NEXT
- Verifier on this file, the results doc and the handoff; handoff with receipts; tag `v0.1.0` on a green `main`; push.

## STATE
- Test media (read-only): `<testfiles>` (174 movie files).
- Pins: DXVKit Plus tag `v0.8.0` (9bc1977), wgpu 30.0.2, egui / egui-wgpu / egui-winit 0.36.2, winit 0.30.13, sha2 0.11.1; rustc 1.99.1 (Homebrew, the build), 1.100.0 (rustup stable, what CI uses; clippy checked with both).
- Gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --release -- -D warnings`, `cargo test --workspace --release`, `cargo deny check`, `cargo build --release`.
- Test support (movie writer, random-block frames of every variant): `tests/support/movie.rs`, included with `#[path]`.
- GPU tests: `glance_gpu::verify::test_gpu` — skip without a GPU or with `GLANCE_SKIP_GPU=1`; `GLANCE_REQUIRE_GPU=1` makes a missing GPU a failure.
- The Swift cross-test program is not committed (session scratch folder); what it calls is written in the handoff.

## OPEN
- [ ] verifier, handoff, tag v0.1.0

## IDEAS
- Skip the Alpha Type pass for Premultiplied clips (hand egui DXVKit Plus's output directly).
- Use the `.crate-meta.json` sidecar found in the test folder as the first cross-test fixture for `glance-library`.
- Measure the scaled picture against Swift Glance's renderer (filter-then-premultiply, HQ chroma nearest) to put a number on that difference.
- A CPU reference for the OS-decoder path (the backend's `cpu_rgba`) so native clips get a tolerance row instead of "shown".
