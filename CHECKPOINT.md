# glance-core CHECKPOINT

Wave: v032-g2-fixes (patch, version 0.3.2; tag withheld, see NEXT). Brief: ~/claude-handoffs/glance-core_2026-11-16_v032-g2-fixes_brief.md (with ~/claude-handoffs/glance-core_2026-11-16_g2-findings_brief-r2.md). Machine: <host> (Apple M5 Max, macOS 27.0.1). Previous wave: v031-g1-fixes (closed, tag v0.3.1; its checkpoint is in git history at 871d0f7).

## DONE
- SCOPE read: both briefs; glance-rs g2-runs evidence and tools (copied with `cp -p` to the scratchpad); winit 0.30.13 `window_delegate.rs:367-428`; winit master (`winit-appkit`, NSDragOperation returns since #4571, no 0.30.x backport); muda 0.21.2 macOS (every submenu `setAutoenablesItems(false)`); wgpu-hal 30.0.2 metal present; Swift Glance a9dc601 `GlanceApp.swift`, `VideoHostingView.swift:274-285`, `FrameClock.swift`, the running Swift reference build (lldb: Window menu items, window `tabbingMode` 2, `collectionBehavior` 0x10080, excluded from the Window menu).
- DIAGNOSE 1, the drop crash (all runs in `docs/data/v032/`): RED v0.3.1 sample 4/10 crashed. The winit hypothesis is false on reading (`cset w0; ret`: x0 is 0 or 1) and by run (NSDragOperation-patched winit, sample: 4/20). Cause located in macOS: plain winit (no Metal, no egui, no core code) 0/20 idle, 0/20 with 16–512 B heap churn, **4/20 with 512–1024 B churn**; file-URL reading as Swift does: 2/20; wgpu surface on demand 0/20, Fifo 2/20, Immediate 7/20, presentsWithTransaction 9/20. Fault: `AESizeOfFlattenedDesc(DragInfo+0x74)` in `CoreDragTerminate ← RemotePortInvalidationCallback` (`lldb-crash.txt`); victims trap on corrupted xzone free lists; Safari crashed with the same signature during the early runs (`safari-crash-summary.txt`). Contradicts the brief (fix in the core / winit) → crash item STOPPED for Amrit.
- Incident: the first diagnosis runs posted drags without checking their target; drops landed on Amrit's browsers and on crash dialogs and Amrit lost browser work. Since then every drop goes through `probes/safe-drag.swift` (drops only on the app under test, else aborts) and `safe-drop.sh` (stops at the first problem, dismisses only the test app's crash dialog). macOS 27 ignores `com.apple.CrashReporter DialogType none`; the key was restored to absent.
- DIAGNOSE 2, menus: RED diff of v0.3.1 vs Swift (`menus-red-diff.txt`): the six differences plus Swift's second Window-menu separator.
- IMPLEMENT: File ▸ Close; muda's Toggle Full Screen removed (AppKit adds Enter Full Screen); window `tabbingMode` disallowed, excluded from the Window menu, `collectionBehavior` as Swift's; Edit and Window menus validated by AppKit (`autoenablesItems`); Edit ▸ Delete and the second separator (macOS); Help ▸ "<app> Help" ⌘? (`showHelp:`); `glance_platform::alert` gives Esc to the second of two buttons; parity Esc line; README; version 0.3.2.
- BUILD + TEST (macOS): menu dump of the bundled sample equals Swift's but for the app name (`menus-green-diff.txt` empty); Help shows "Help isn't available for …" in both (`help-*.txt`); Esc on the two-button alert: v0.3.1 nothing (`red-alert-esc.txt`), v0.3.2 → 1 "Not Now", Return → 0 (`green-alert-*.txt`). Gates: fmt, clippy -D warnings, `GLANCE_REQUIRE_GPU=1` tests 71 passed 0 failed, deny ok, build ok. Probes deleted (sources kept in `docs/data/v032/probes/`).

- Committed 8c90e4d on main, pushed; CI run 43902765934 green (lint, macos, windows). Verifier on the handoff draft: 49 confirmed / 14 unproven / 6 wrong. All six wrong ones corrected: winit history, which runs were safe, the three alive-but-not-opened tries, the repro steps, the stale RED diff (regenerated), and a stale `com.amritus.glancecore.swiftref` LaunchServices record from 2026-11-15 (unregistered). Session-only evidence saved to `docs/data/v032/session-evidence.txt`. Handoff `docs/handoffs/v032-g2-fixes.md` (STATUS BLOCKED), mailbox copy, note for glance-rs `~/claude-handoffs/glance-rs_2026-11-16_note-from-glance-core-v032.md`.

## NEXT
- Amrit decides the crash (one line in the handoff): tag v0.3.2 without a crash fix and report the CoreDrag bug to Apple, or hold the tag. The drop crash cannot reach 0/20 from the core without his call.

## STATE
- Drop fixtures: the repo's own `tests/fixtures/tiles_dxt1.mov`, `tiles_hap1.mov`, `tiles_dxt5.mov` (copies with `cp -p` outside ~/Documents for the Swift build, which asks for Documents access; that prompt was answered "Don't Allow").
- Pins: DXVKit Plus `v0.11.0`; wgpu 30.0.x (wgpu-hal 30.0.2), egui 0.36.2, winit 0.30.13, muda 0.21.2.
- Throw-away ids only: `com.amritus.glancecore.sample` (prompt marked shown), `com.amritus.glance.swiftref` (scratch build, unregistered after). One LaunchServices record left: `/Applications/Glance Sample.app` (v0.3.1).
- GUI runs only with Amrit's go-ahead and the safe tools (memory: gui-input-tests-need-free-machine).

## OPEN

## IDEAS
- Present timestamps from the OS (CAMetalLayer presented time) for the A/V estimate instead of "one refresh after drawing".
- A camera + microphone A/V check (flash and click clip) to measure the absolute offset.
- Stream the audio instead of decoding the whole track for very long clips.
- Windows TaskDialog with Swift's button names once the C4 installer brings the comctl32 v6 manifest.
