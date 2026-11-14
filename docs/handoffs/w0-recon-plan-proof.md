project:    glance-core
pass:       w0-recon-plan-proof
STATUS:     COMPLETE
parent:     NONE (new repo)
result:     f4ea06a, a5f7f02
session:    72473235-2ffb-45d5-bcaf-f4b12d0f8330
written_at: 2026-11-14T17:51Z

# GLANCE-CORE W0 — RECON, PLAN, PROOF: RESULT

## What shipped
- **Repo** `AmritusG/glance-core` (private), `main`, at `<workspace>/glance-core`. Seeded: `CLAUDE.md`, `CHECKPOINT.md`, `docs/00-ROADMAP.md`, `docs/01-RESEARCH.md`, `docs/02-PARITY.md`, `docs/03-W0-RESULTS.md`.
- **Recon**, a source line per fact, in `docs/research/` (6,971 lines): Glance, CueGlance, GlEnc, MaCrate + iCrate + SendEhrKit, MaCrate's database and every other persistent format, and the cross-platform backends from primary docs with licences. `docs/01-RESEARCH.md` is the synthesis: 9 contradictions with the brief, the critical rules, bundle ids / settings / release pipelines, the MaCrate data plan and cross-test, the existing Rust code and who owns what, 11 hard spots with options, 16 decisions for Amrit.
- **Parity checklists** (`docs/02-PARITY.md`): 457 items — core 30, Glance 94, CueGlance 95, GlEnc 112, MaCrate 126 — the gate before a Rust app replaces its Swift twin.
- **Roadmap** (`docs/00-ROADMAP.md`): repos under `native-builds/rust/`, the core's crates, what GlancEhr needs from the core, how apps pin the core (git tag; DXVKit Plus pinned by the core), waves in the agreed order, the replacement step, the Windows test plan, known limits.
- **The proof** (the only code): four thin crates over DXVKit Plus 0.7.1 — `glance-media`, `glance-gpu`, `glance-playback`, `glance-ui` — and `glance-proof`: clips side by side in an egui 0.36 window on wgpu 30 / Metal, with the per-clip Alpha Type switch, the shared checkerboard and frame timing logged.
- **Briefs for other repos** (not started; Amrit decides): `DXVKit-Plus_2026-11-14_glance-family-needs_brief.md`, `loombda-rs_2026-11-14_dxvkit-plus-dedupe_brief.md`.

## The main finding about the existing Rust
DXVKit Plus already does what the brief's example crate list wanted the core to do (media-io, decode-hap, decode-native, playback): demux, every HAP and DXV variant, native macOS decode zero-copy, an exact-time transport, a read-ahead cache and BC upload. So the core is a thin layer over it and carries **no decoder** — that is how "no duplicate decoders across repos" is kept. What DXVKit Plus lacks for the family is in its brief: Glance's 0-difference render path for DXT1 / DXT5 / HAP, the Windows backend, edit lists, audio, encoders. The encoders and a QuickTime writer exist today only in loombda-rs (proprietary, Amrit's own).

## Validated (number per claim; release build, M5 Max, 120 Hz, 15.0 s measured per run)
- Gate run A — `Metric_01_HapM.mov` (HAP Q Alpha, 1920x1080, 25 fps) + `DXT5-DXV3.mov` (DXV3 Normal Quality With Alpha, 1920x1080, 30 fps): 376 and 451 frames shown, **0 skipped**, shown fps 25.06 / 30.06; window present interval mean 8.350 ms, p50 8.34, p99 8.58, max 17.00 (4 of 1,797 refreshes over 12 ms); UI-thread wait for a frame p99 0.018 / 0.017 ms; upload + convert CPU p50 0.92 / 0.64 ms.
- Runs B–D (HAP 4 chunks + DXV High Quality With Alpha; HAP Q Alpha 3840x2160 + DXV Normal Quality; ProRes 4444 3840x2160 through VideoToolbox + DXV NQ Alpha): 0 skipped in every clip; present p99 8.56–8.60 ms.
- The picture on the GPU against DXVKit Plus's CPU decode of the same frame (read-back at the end of each run): max difference 2 (HAP Q Alpha 1080p, DXV DXT5), 1 (HAP, DXV DXT1, HAP Q Alpha 2160p), **0** (DXV High Quality With Alpha).
- 10 tests pass (3 on the real GPU, each with a negative control); fmt, clippy `-D warnings`, release build clean; one wgpu (30.0.2) in the tree.
- Files and all numbers: `docs/03-W0-RESULTS.md`; raw `docs/data/w0-proof-{a,b,c,d}.{json,csv}`.
- Test media, read-only, not copied: `<testfiles>` (the `<testfiles>` the Swift repos name is not on this Mac).

## Contradictions with the brief (rule 7 said STOP; see Deviations)
1. **iCrate does not share MaCrate's database.** It builds an in-memory store from the sidecars on the drive (`crate/iPad/iCrate/iCrate/SearchIndex.swift:123-125`). The Mac–iPad contract is the sidecar `.<clip>.crate-meta.json`, `.crate-stashes.json` and the `clip_id` hash. MaCrate's SQLite is a cache ("sidecars are canonical").
2. "HapM premultiplied vs Hap5 straight" is superseded: both straight since Glance 0.18.0 (Amrit, 2026-10-10, `glance/CLAUDE.md:19`).
3. "DXV3 pads width only" is stale: 16 on both axes for every variant since DXVKit.
4. "Viewers default Premultiplied with a per-clip switch" is true for DXV alpha only; HAP alpha has no switch.
5. No Metal shaders on the Mac: Glance, CueGlance and MaCrate render with OpenGL 3.2 (GLSL); Metal is iOS-only.
6. MaCrate's migrations are raw SQL (12), no Date / UUID columns, no custom collations — simpler than feared.
7. Glance has no Quick Look extension on `main`; MaCrate has no Finder extension ("Send to Arena from Finder" is SendEhr's).
8. CueGlance is a Resolume envelope-preset editor, not a player with cue features; its README is stale.
9. The GlancEhr scope brief is not in `~/claude-handoffs` on this Mac.

## Deviations
- **Did not stop at the contradictions.** None of them touches the proof or the core's design, and W0 ends in STOP-and-report anyway; the wave was finished and each contradiction is left undecided in `docs/01-RESEARCH.md` §0, with the plan following the code. If the STOP was meant literally, the cost is the roadmap's MaCrate section, which is written for both readings.
- The brief's example crate list (decode-hap, decode-native, …) was not followed: those would duplicate DXVKit Plus. Crates for audio, library-db, arena-client, settings are planned, not created (no empty stubs).
- egui host is winit + egui-winit + egui-wgpu (as loombda-rs), not eframe; egui pinned 0.36.2 (as loombda-rs) though 0.37.0 exists.
- Recon was done by five reader agents (one per app area + backends), each writing its research file; the main session read the Rust repos itself and re-read in source the claims that contradict the brief. Every `path:line` was machine-checked to exist; the per-app feature inventories were not re-read line by line.
- Extra runs B–D and the GPU-vs-CPU picture check were added beyond "one HAP and one DXV".
- An earlier build of the proof showed 1.6–4 % double-length refreshes while five agents were running; the final build shows 0–4 per 1,800. Cause not isolated; no clip frame was skipped in either. Noted in the results doc.
- Licence field of the new repo set to `LicenseRef-Proprietary` (private repo) pending decision 7.

## Not verified
- **Nobody has looked at the proof window.** The picture is verified by read-back, not by eye. Ten seconds of Amrit's eyes: `cd <workspace>/glance-core && ./target/release/glance-proof "<hap>.mov" "<dxv>.mov"` (stays open; Alpha Type and Background are live).
- Windows: nothing (by the rule). Audio, seeking, scrubbing: not in W0.
- MaCrate's final schema was derived by replaying the migration SQL, not compared with a file the Swift app wrote.

## Needs Amrit (the first six; all 16 in `docs/01-RESEARCH.md` §9)
1. iCrate compatibility = sidecars + stash file + `clip_id`, not the SQLite file — confirm; keep the SQLite format too for the swap? (recommended: yes)
2. ProRes on Windows: none / ask Apple / own decoder / a young crate / Mac only. (recommended: none at first, ask Apple in parallel)
3. Keep Glance's 0-difference render bar as the gate (DXVKit Plus then builds the exact path for DXT1 / DXT5 / HAP), or accept the GPU's BC decode (≤ 2 / 255)? (recommended: keep)
4. May loombda-rs's encoders and QuickTime writer move into DXVKit Plus under MIT? (GlEnc-rs needs them)
5. SendEhrKit is GPL-3.0-or-later: may the Rust Arena client be written from it under the core's licence?
6. Windows signing: Microsoft's Artifact Signing serves individuals in the US and Canada only, organisations in the EU — company, or a CA certificate on a token?

## Amrit's answers given during this session
None — no question was asked and no answer given; the session ran from the brief alone.

## Independent check
`cc-kit:verifier` re-derived 64 claims of the checkpoint, the results tables (every cell recomputed from the CSV / JSON) and `01-RESEARCH.md` §0 and §6 from sources: **54 confirmed, 6 wrong, 4 unproven**. The six wrong were corrected in commit `a5f7f02` (a mean rounded 0.207 for 0.206; "0.05" for 0.06–0.07; "GlEnc has no GPU code" — it has an OpenGL preview layer; "iCrate Metal is DXV only" — it also shows HAP through Metal; glance is 3 commits past v0.18.1, not 2; a stale OPEN line). Unproven, and stated as such in the docs: the runs' exit codes are not in the data files (they were printed 0 in the session); the earlier-build stutter figures have no data file; nobody has looked at the window. It also re-checked 2,356 `path:line` citations in the research files: all resolve. Side note from the verifier: the W0 brief's own footer does not match its body (footer `501dad51 8178`, recomputed `932fb3fb 7793`); the brief was read whole to its END line.

## Commits
- `f4ea06a` W0: recon, roadmap, parity checklists and the HAP + DXV proof
- `a5f7f02` W0: verifier corrections, checkpoint closed
Both on `main`, pushed to private `AmritusG/glance-core`. This handoff is committed after them.

## RECEIPTS
```
$ git log -2 --format="%H %ad %s" --date=iso
a5f7f0232ded6b213df74ff8ca521b3d6c5faa58 2026-11-14 18:50:41 +0100 W0: verifier corrections, checkpoint closed
f4ea06a1dacbce0ed736b315f5bdd441133b34f0 2026-11-14 18:39:47 +0100 W0: recon, roadmap, parity checklists and the HAP + DXV proof
$ git status -sb
## main...origin/main
$ git ls-remote origin main
a5f7f0232ded6b213df74ff8ca521b3d6c5faa58	refs/heads/main
$ gh repo view AmritusG/glance-core --json visibility -q .visibility
PRIVATE
$ cargo fmt --all --check; echo exit=$?
exit=0
$ cargo clippy --workspace --all-targets --release -- -D warnings
    Finished `release` profile [optimized + debuginfo] target(s) in 0.29s
$ cargo test --workspace --release | grep -E "^test |test result: ok. [1-9]"
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test straight_colour_is_multiplied_by_its_alpha ... ok
test premultiplied_colour_goes_out_as_stored ... ok
test the_alpha_switch_recomposites_without_a_new_frame ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test tests::dxv_alpha_is_the_users_choice_and_hap_alpha_is_straight ... ok
test tests::a_dxv_variant_is_read_from_the_planes ... ok
test tests::hap_is_named_by_fourcc_and_unknown_codecs_keep_theirs ... ok
test tests::a_missing_file_is_an_error_not_a_panic ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test tests::cadence_counts_showings_and_skips_across_the_loop_point ... ok
test tests::percentiles_are_nearest_rank ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test tests::a_picture_fits_the_space_with_its_aspect_kept ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
$ cargo build --release
    Finished `release` profile [optimized + debuginfo] target(s) in 0.07s
$ cargo tree -i wgpu | head -1
wgpu v30.0.2
$ ls -la docs docs/data docs/research
docs:
total 232
drwxr-xr-x   9 <user>  staff    288 14 Nov 18:41 .
drwxr-xr-x  16 <user>  staff    512 14 Nov 18:39 ..
-rw-r--r--   1 <user>  staff  15352 14 Nov 18:38 00-ROADMAP.md
-rw-r--r--   1 <user>  staff  36829 14 Nov 18:50 01-RESEARCH.md
-rw-r--r--   1 <user>  staff  57111 14 Nov 18:33 02-PARITY.md
-rw-r--r--   1 <user>  staff   7204 14 Nov 18:50 03-W0-RESULTS.md
drwxr-xr-x  10 <user>  staff    320 14 Nov 18:31 data
drwxr-xr-x   2 <user>  staff     64 14 Nov 18:41 handoffs
drwxr-xr-x   8 <user>  staff    256 14 Nov 18:35 research

docs/data:
total 480
drwxr-xr-x  10 <user>  staff    320 14 Nov 18:31 .
drwxr-xr-x   9 <user>  staff    288 14 Nov 18:41 ..
-rw-r--r--   1 <user>  staff  56043 14 Nov 18:30 w0-proof-a.csv
-rw-r--r--   1 <user>  staff   2789 14 Nov 18:30 w0-proof-a.json
-rw-r--r--   1 <user>  staff  54834 14 Nov 18:31 w0-proof-b.csv
-rw-r--r--   1 <user>  staff   2877 14 Nov 18:31 w0-proof-b.json
-rw-r--r--   1 <user>  staff  55753 14 Nov 18:31 w0-proof-c.csv
-rw-r--r--   1 <user>  staff   2810 14 Nov 18:31 w0-proof-c.json
-rw-r--r--   1 <user>  staff  55754 14 Nov 18:31 w0-proof-d.csv
-rw-r--r--   1 <user>  staff   2606 14 Nov 18:31 w0-proof-d.json

docs/research:
total 1360
drwxr-xr-x  8 <user>  staff     256 14 Nov 18:35 .
drwxr-xr-x  9 <user>  staff     288 14 Nov 18:41 ..
-rw-r--r--  1 <user>  staff   94421 14 Nov 18:36 backends.md
-rw-r--r--  1 <user>  staff   93980 14 Nov 18:33 cueglance.md
-rw-r--r--  1 <user>  staff  101402 14 Nov 18:21 glance.md
-rw-r--r--  1 <user>  staff  130925 14 Nov 18:27 glenc.md
-rw-r--r--  1 <user>  staff   82361 14 Nov 18:23 macrate-db.md
-rw-r--r--  1 <user>  staff  182950 14 Nov 18:31 macrate.md
$ ls -la target/release/glance-proof
-rwxr-xr-x  1 <user>  staff  13998144 14 Nov 18:30 target/release/glance-proof
$ ls -la ~/claude-handoffs | grep -E "DXVKit-Plus_2026-11-14|loombda-rs_2026-11-14"
-rw-r--r--    1 <user>  staff    8739 14 Nov 18:39 DXVKit-Plus_2026-11-14_glance-family-needs_brief.md
-rw-r--r--    1 <user>  staff    4571 14 Nov 18:39 loombda-rs_2026-11-14_dxvkit-plus-dedupe_brief.md
```
Proof runs (session output, final binary; the same numbers are in docs/data/w0-proof-*.json):
```
a exit=0
b exit=0
c exit=0
d exit=0
no orphan            (pgrep -x glance-proof after the runs)
run a: build=release measured=15.00s refresh=120.0 presents=1797 interval mean=8.350 p50=8.34 p95=8.50 p99=8.58 max=17.00 long(>12ms)=4
   Metric_01_HapM.mov | HAP Q Alpha 1920x1080 25.000fps n=201 alpha=Straight | shown=376 skipped=0 fps=25.06 | fetch p50=0.008 p99=0.018 max=0.02 | show p50=0.92 p99=1.45 max=1.56 | miss=1 hits=1908 err=None werr=0
        picture vs CPU: frame 198 max_diff=2 mean_diff=0.1480 mean_rgba=[85.9, 48.4, 50.5, 255.0]
   DXT5-DXV3.mov | DXV Normal Quality With Alpha 1920x1080 30.000fps n=300 alpha=Premultiplied | shown=451 skipped=0 fps=30.06 | fetch p50=0.007 p99=0.017 max=0.02 | show p50=0.64 p99=1.11 max=1.22 | miss=1 hits=1908 err=None werr=0
        picture vs CPU: frame 179 max_diff=2 mean_diff=0.0891 mean_rgba=[17.5, 10.0, 25.9, 27.8]
run b: presents=1800 interval p50=8.33 p99=8.60 max=8.74 long=0 | HAP (4 chunks) 1080p30 shown=451 skipped=0 picture max_diff=1 | DXV High Quality With Alpha 1080p25 shown=376 skipped=0 picture max_diff=0
run c: presents=1800 interval p50=8.34 p99=8.59 max=8.68 long=0 | HAP Q Alpha 2160p30 shown=451 skipped=0 show p50=2.60 picture max_diff=1 | DXV Normal Quality 1080p30 shown=451 skipped=0 picture max_diff=1
run d: presents=1800 interval p50=8.33 p99=8.56 max=8.72 long=0 | ProRes 4444 (ap4h) 2160p30 native shown=451 skipped=0 picture n/a | DXV Normal Quality With Alpha 1080p30 shown=451 skipped=0 picture max_diff=1
```
END OF HANDOFF w0-recon-plan-proof sha256:220dd0fc 15625
