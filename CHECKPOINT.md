# glance-core CHECKPOINT

Wave: c1-exact-picture. Brief: ~/claude-handoffs/glance-core_2026-11-14_c1-exact-picture_brief.md. Machine: <host> (Apple M5 Max, macOS 27.0.1).

## DONE
- Read in full: the brief, `~/claude-handoffs/glance-core_2026-11-14_note-from-Cowork-w0-review.md`, `~/claude-handoffs/glance-core_2026-11-14_note-from-DXVKit-Plus-w1-exact-path.md`, roadmap, parity (Core + Glance routing / exactness / alpha), research §0 §3 §9. `.ref/DXVKit-Plus` refreshed to `v0.8.0` (9bc1977).
- Diagnosis read in source (Swift Glance a9dc601, DXVKit fb506da = v1.0.0 + docs, DXVKit Plus v0.8.0): routing order and message wording, `clipKey` layout, alpha-look probe, the exactness bar and where the Alpha Type pass sits, the scaled-filter model, the HAP Q formulas. Goes into `docs/handoffs/c1-exact-picture.md`.
- Re-pin: `dxvkit-plus-{core,wgpu,apple}` tag `v0.8.0` (Cargo.lock 9bc1977c). No HAP Q hash was pinned in this repo from 0.7.1 (none to re-derive).
- `glance-gpu`: `ClipView` on `BlockPath::Exact` by default, `with_path` / `set_path` / `path` for a grid; the present pass in whole code values (as stored / straight = round(c·a/255) / alpha-only = white at that alpha, as Swift Glance shows HapA); `expected_picture` (the CPU reference); `verify` module (the harness).
- `glance-media`: `route` (content routing over DXVKit Plus's probe, Swift Glance's messages), `Clip::open`, `first_video_fourcc`, `display_name`, `clip_key`, `alpha_report` / `alpha_look_count` / `alpha_look`. `ClipPlayer::open` routes through it.
- Fixtures committed from DXVKit Plus v0.8.0 with `cp -p` (CC0 tile movies, synthetic HAP R / HDR, DXVKit's MIT vector streams): `tests/fixtures/`, 1.2 MB.
- Harness, `cargo test --release -p glance-gpu` (7 + 3 tests pass): every variant (12) at 9 sizes 1x1 … 3840x2160, each Alpha Type — 180 rows, max 0; 36 fixture files, 67 rows, 563 frame comparisons, max 0; 64 DXVKit vector pictures hash to DXVKit's reference RGBA; flipped-bit and wrong-Alpha-Type controls; sampled path measured 1–3 and told apart; HAP HDR equals DXVKit Plus's GPU conversion; OS-codec fixtures route Native and show.

## NEXT
- Swift cross-test harness (clipKey + probe + FourCC names on fixtures and Amrit's clips), test data committed.
- glance-media tests: routing, messages, hostile files.
- glance-ui headless stage + scaled (1.5x / 0.5x) and checkerboard tests.
- `glance-proof verify` over Amrit's clips; HAP Q measurement vs Swift's convention; `docs/04-C1-RESULTS.md`, `docs/data/c1-*`.
- Licence MIT, docs (roadmap, parity, research §9), CI, deny; verifier; handoff; tag v0.1.0.

## STATE
- Test media (read-only): `<testfiles>`.
- Pins: DXVKit Plus tag `v0.8.0` (9bc1977), wgpu 30.0.2, egui / egui-wgpu / egui-winit 0.36.2, winit 0.30.13, sha2 0.11.1, rustc 1.99.1.
- Gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --release -- -D warnings`, `cargo test --workspace --release`, `cargo deny check`, `cargo build --release`.
- Test support (movie writer, random-block frames of every variant): `tests/support/movie.rs`, included with `#[path]`.
- GPU tests skip without a GPU unless `GLANCE_REQUIRE_GPU=1`.

## OPEN
- [ ] Swift cross-test (clipKey, probe)
- [ ] glance-media routing / message / hostile-file tests
- [ ] headless stage, scaled-filter and checkerboard tests
- [ ] Amrit's clips through the harness; HAP Q measurement
- [ ] Licence, README, ATTRIBUTION, roadmap, parity, research §9
- [ ] CI + cargo deny, green on main
- [ ] results doc, verifier, handoff, tag v0.1.0

## IDEAS
- Skip the Alpha Type pass for Premultiplied clips (hand egui DXVKit Plus's output directly).
- Use the `.crate-meta.json` sidecar found in the test folder as the first cross-test fixture for `glance-library`.
