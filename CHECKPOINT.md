# glance-core CHECKPOINT

Wave: w0-recon-plan-proof. Brief: ~/claude-handoffs/glance-core_2026-11-14_w0-recon-plan-proof_brief.md. Machine: <host> (Apple M5 Max, macOS 27.0.1).

## DONE
- Brief read in full; repo initialised (`main`), `.ref/` + `target/` gitignored.
- 14 reference repos cloned read-only into `.ref/` (glance, glance-releases, cueglance, cueglance-releases, glenc, glenc-public, glenc-releases, crate, CrateRelease, SendEhr, DXVKit-Plus, dxvkit, loombda-rs, swift-snappy). `git status` clean in each (reader agents' reports).
- Recon extracted with a source line per fact: `docs/research/glance.md` (972 lines), `cueglance.md` (1,103), `glenc.md` (1,473), `macrate.md` (1,141), `macrate-db.md` (1,063), `backends.md` (1,219). 6,971 lines.
- Existing Rust read by the main session (DXVKit Plus 0.7.1 @ 09f1dbc, loombda-rs @ b7e6f68): DXVKit Plus owns demux, HAP, DXV, native macOS decode, transport and wgpu upload; the core is a thin layer over it. Ownership table: `docs/01-RESEARCH.md` §6.
- `docs/01-RESEARCH.md` (383 lines): synthesis, 9 contradictions with the brief (§0), critical rules, MaCrate data + cross-test plan, existing Rust, backends, 11 hard spots, 16 decisions for Amrit.
- `docs/02-PARITY.md` (622 lines): 457 checklist items — core 30, Glance 94, CueGlance 95, GlEnc 112, MaCrate 126.
- `docs/00-ROADMAP.md`: repos, crates, GlancEhr coverage, pins, waves, the replacement step, the Windows test plan, known limits.
- Workspace: `glance-media`, `glance-gpu`, `glance-playback`, `glance-ui`, `glance-proof`. One wgpu (30.0.2) across DXVKit Plus 0.7.1 and egui 0.36.2 (`cargo tree -i wgpu`).
- Gates on the final tree: `cargo fmt --all --check` ok; `cargo clippy --workspace --all-targets --release -- -D warnings` ok; `cargo test --workspace --release` 10 passed, 0 failed; `cargo build --release` ok.
- Proof, final binary, release, 120 Hz, 15.0 s measured per run (`docs/03-W0-RESULTS.md`, raw `docs/data/w0-proof-{a,b,c,d}.{json,csv}`): run A `Metric_01_HapM.mov` (HAP Q Alpha 1080p25) + `DXT5-DXV3.mov` (DXV3 NQ Alpha 1080p30): 376 and 451 frames shown, 0 skipped, present interval p50 8.34 ms / p99 8.58 ms, picture vs CPU decode max 2 / 255. Runs B–D (HAP chunked + DXV HQ Alpha; HAP Q Alpha 2160p + DXV NQ; ProRes 4444 2160p native + DXV NQ Alpha): 0 skipped in every clip, picture vs CPU max 0–1.
- Briefs for other repos: `~/claude-handoffs/DXVKit-Plus_2026-11-14_glance-family-needs_brief.md`, `~/claude-handoffs/loombda-rs_2026-11-14_dxvkit-plus-dedupe_brief.md`.

## NEXT
- Amrit rules on the contradictions and decisions in `docs/01-RESEARCH.md` §0 and §9 (first: iCrate shares sidecars, not the database; ProRes on Windows; the 0-difference render bar; the loombda-rs encoders under MIT).
- Then core wave C1 (exact picture) — needs DXVKit Plus brief item 1 or a relaxed bar. No brief is queued until Amrit decides.

## STATE
- Test media (read-only): `<testfiles>` (`<testfiles>` does not exist on this Mac).
- GlancEhr scope brief: not in `~/claude-handoffs` on this Mac.
- Pins: DXVKit Plus tag `v0.7.1` (09f1dbc), wgpu 30.0.2, egui / egui-wgpu / egui-winit 0.36.2, winit 0.30.13, rustc 1.99.1.
- Gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --release -- -D warnings`, `cargo test --workspace --release`, `cargo build --release`.
- Proof: `./target/release/glance-proof <clip>... [--seconds N] [--warmup N] [--out f.json] [--log f.csv]`; without `--seconds` it stays open.
- Not verified by eye: nobody has looked at the proof window; the picture is checked by GPU read-back against the CPU decode.
- Alpha rule in code: DXV alpha = per-clip Alpha Type (default Premultiplied); HAP alpha = straight; frames decoded as stored. Checkerboard 8 pt, greys 0.85 / 0.65.

## OPEN
- [ ] GitHub repo created, committed, pushed
- [ ] Verifier run on this checkpoint
- [ ] Handoff written, committed and copied to the mailbox

## IDEAS
- A `glance-proof --headless` mode that renders offscreen and hashes frames, for CI without a window server.
- Skip the Alpha Type pass for Premultiplied clips (hand egui DXVKit Plus's output directly).
- Use the `.crate-meta.json` sidecar found in the test folder as the first cross-test fixture for `glance-library`.
