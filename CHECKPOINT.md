# glance-core CHECKPOINT

Wave: 60hz-and-v033 (patch, version 0.3.3; open). Brief: ~/claude-handoffs/glance-core_2026-11-16_60hz-run-and-v033-show-all_brief.md (part B: ~/claude-handoffs/glance-core_2026-11-16_app-menu-show-all_brief-r2.md). Machine: <host> (Apple M5 Max, macOS 27.0.1). Previous wave: v032-g2-fixes (closed, tag v0.3.2 on 8c90e4d; its checkpoint is in git history at 6bdc3d9).

## DONE
- SCOPE read: both briefs, `docs/05-C2-RESULTS.md`, `tools/c2-measure.sh`, glance-rs `tools/swift-ref.sh` / `g2-runs.sh` menus section, `finish_app_menus`.
- PART A, real 60 Hz: projector Panasonic PT-RZ570 (CG display 3, winit "Monitor #53534"), 1920x1200, 60.00 Hz, settings untouched. New proof option `--display <name>` (glance-ui `Host::move_to_display`, `display_name`; the report records the display); `tools/c2-measure.sh` takes paces and a display, logs CSV per refresh. Waited for another session's tl-play run to end. 26 runs (C2's 13 x 2 passes, pace 1): **0 skipped, 11,139 shown**, presents p50 16.67 ms, max 27.56 ms; load: Arena 25–46 %, another session's rustc builds and Time Machine during about half the runs. `docs/05-C2-RESULTS.md` §1a, `docs/data/v033-60hz/`. Parity item ticked (60 Hz met), roadmap C2 row updated.

- PART B RED: bundled sample (scratch copy of `/Applications/Glance Sample.app`, v0.3.2 menu code, ad-hoc signed) vs Swift Glance a9dc601 (`com.amritus.glance.swiftref`, scratch build), both `open -a` with `tiles_dxt5.mov` outside ~/Documents, no app hidden: the one difference is Show All (Swift off, ours on) (`docs/data/v033/menus-red-*.txt`).
- PART B fix: `finish_app_menus` turns AppKit validation on for the app menu (found by its `unhideAllApplications:` item); About / Set … as Default / Quit stay on (muda items are their own target). Version 0.3.3, README.
- PART B GREEN: no app hidden, each app frontmost through its dump: equal but for the app name (`menus-green2-diff.txt` empty). Sample-only A/B, 5 + 5 alternating: Show All off 5/5 (v0.3.3) vs on 5/5 (v0.3.2) (`docs/data/v033/ab/`). The Window items (Minimize, Zoom, Move & Resize, Move to PT-RZ570) read off in some dumps of both builds when another app took the front during the dump (green1, green3, A/B runs 3–5): focus state, not this change. Amrit chose "Hide Terminal": with Terminal hidden, Show All on in both, dumps equal but for the app name (`menus-hidden-terminal-*.txt`); Terminal shown again after. Scratch bundles unregistered (`lsregister-after.txt`).
- Gates (working tree on 6bdc3d9): fmt ok, clippy -D warnings ok, `GLANCE_REQUIRE_GPU=1` tests 71 passed 0 failed, deny ok, build ok (`docs/data/v033/gates.txt`).

## NEXT
- Commit + push, CI green, tag v0.3.3, note for glance-rs, handoff `docs/handoffs/60hz-and-v033.md` + mailbox copy.

## STATE
- Pins: DXVKit Plus `v0.11.0`; wgpu 30.0.x (wgpu-hal 30.0.2), egui 0.36.2, winit 0.30.13, muda 0.21.2.
- The 2160p DXV clips of C2 are in the 2026-11-15 session scratchpad `<scratchpad>/synth/` (not in the repo).
- One LaunchServices record left: `/Applications/Glance Sample.app` (v0.3.1).
- GUI runs only with Amrit's go-ahead and the safe tools (memory: gui-input-tests-need-free-machine).

## OPEN
- The drop crash (macOS CoreDrag): Apple report sent by Amrit 2026-11-16; no core work owed.

## IDEAS
- Present timestamps from the OS (CAMetalLayer presented time) for the A/V estimate instead of "one refresh after drawing".
- A camera + microphone A/V check (flash and click clip) to measure the absolute offset.
- Stream the audio instead of decoding the whole track for very long clips.
- Windows TaskDialog with Swift's button names once the C4 installer brings the comctl32 v6 manifest.
