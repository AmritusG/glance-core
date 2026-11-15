# glance-core CHECKPOINT

Wave: v031-g1-fixes (patch, tag v0.3.1). Brief: ~/claude-handoffs/glance-core_2026-11-15_v031-g1-fixes_brief.md (with ~/claude-handoffs/glance-core_2026-11-15_g1-findings_brief.md and ~/claude-handoffs/glance-core_2026-11-15_folder-listing_brief.md). Machine: <host> (Apple M5 Max, macOS 27.0.1). Previous wave: c3-shell (closed, tag v0.3.0; its checkpoint is in git history at ae8930d).

## DONE
- SCOPE read: the three briefs; winit 0.30.13 macOS `view.rs` keyDown / `app.rs` sendEvent; rfd 0.18 macOS + Windows message dialog; Apple docs (NSEvent local monitor, NSMenu performKeyEquivalent:, NSAlert runModal, localizedStandardCompare:), Microsoft docs (StrCmpLogicalW, TaskDialogIndirect: comctl32 v6 only); Swift Glance a9dc601 DefaultHandlerControl.swift:80-147, PlayerModel.swift:233-268. Sources in `docs/handoffs/v031-g1-fixes.md` READ.
- DIAGNOSE (RED on installed v0.3.0): bare Space ×2 + A → 0 menu commands (`docs/data/v031/red-keys.txt`); alert owned by UserNotificationCenter, outlives a killed app (`red-alert.txt`); default STAGE_DIR → two records, exit 1 (`red-stage.txt`). No contradiction with the brief.
- IMPLEMENT: `glance_platform::install_menu_key_monitor` (NSEvent local monitor → mainMenu performKeyEquivalent:, installed by `run_shell`); `glance_platform::alert` (NSAlert runModal; Windows MessageBoxW owned by the active window), `glance_ui::dialog` API unchanged; kit `lib.sh` resolves STAGE_DIR with `pwd -P`, README says so; `glance_platform::movie_files` / `localized_standard_compare` / `conforms_to_movie` (Windows StrCmpLogicalW + extension list + hidden attribute), never-panicking merge sort `sort_by_os`. Version 0.3.1.
- BUILD + TEST 1: fmt, clippy -D warnings, `GLANCE_REQUIRE_GPU=1 cargo test --workspace --release` 71 passed 0 failed, deny ok, build; Windows clippy of glance-platform clean; iOS check clean; CI green on 435920b (run 43762769333: lint, macos incl. Rosetta load, windows). First CI run (a56690a) failed: the runner's macOS says mkv is not a movie → the type test asserts only what every macOS agrees on.
- BUILD + TEST 2 (installed notarised v0.3.1, from the kit's default STAGE_DIR): release + install one LaunchServices record each (`release.txt`, `install.txt`); Space ×2 → 2 PlayPause, window got 0 Space; A on DXV alpha → 1 toggle, window got 0 A; A on opaque → nothing (`green-keys.txt`); alert is a window of the app's pid, gone after kill (`green-alert.txt`); RED STAGE_DIR script now exit 0 (`green-stage.txt`).
- BUILD + TEST 3: folder tests (`crates/glance-platform/tests/folder.rs`): Finder order on 22 names (digit runs past 2^64 compare modulo 2^64 — recorded), dot file + `chflags hidden` skipped, type test with macOS 27.0.1's set recorded.
- Docs: parity (Space, A, folder navigation, hidden files, alerts), roadmap, README "What 0.3.1 fixes", CLAUDE.md, kit README; handoff `docs/handoffs/v031-g1-fixes.md`.

## NEXT
- Verifier on the handoff, then the closing commit, tag `v0.3.1` on it after its CI run, push; note for glance-rs `~/claude-handoffs/glance-rs_2026-11-15_note-from-glance-core-v031.md`; mailbox copy `~/claude-handoffs/glance-core_2026-11-15_v031-g1-fixes-result.md`.

## STATE
- Test media (read-only): `<testfiles>` (174 movie files).
- Pins: DXVKit Plus `v0.11.0` (29fa9f5); wgpu 30.0.x, egui 0.36.2, winit 0.30.13; rustc 1.99.1 (Homebrew), 1.100.0 (rustup stable, CI and the Intel build).
- Throw-away bundle id for the sample app and the Swift settings harness: `com.amritus.glancecore.sample`. Never `com.amritus.glance`. The sample stays installed at `/Applications/Glance Sample.app` (v0.3.1; removal in the handoff).
- Kit staging: the kit's default `STAGE_DIR` (`$TMPDIR/glance-kit/<id>`, resolved to its physical path by `lib.sh` since v0.3.1); notary profile `amritus-notary`.
- Gates: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --release -- -D warnings`, `cargo test --workspace --release`, `cargo deny check`, `cargo build --release`.

## OPEN

## IDEAS
- Present timestamps from the OS (CAMetalLayer presented time) for the A/V estimate instead of "one refresh after drawing".
- A camera + microphone A/V check (flash and click clip) to measure the absolute offset.
- Stream the audio instead of decoding the whole track for very long clips.
- Windows TaskDialog with Swift's button names once the C4 installer brings the comctl32 v6 manifest.
