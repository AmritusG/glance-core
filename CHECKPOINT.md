# glance-core CHECKPOINT

Wave: covered-window-pacing. Brief: ~/claude-handoffs/glance-core_2026-11-21_from-glancehr_covered-window-pacing.md. Machine: the core's home machine. Previous waves (kit-dmg-record-fix / v0.9.1, v0.9.2 from the GlancEhr session, and before): closed; their checkpoint is in git history.

## DONE
- READ: GlancEhr's fix (b2e7d78, crates/glancehr/src/app.rs `frame`: a frame within 2 ms of the last sleeps out 0.95 of a refresh) and its w6 evidence; glance-ui's loop (lib.rs Runner / Running::redraw, output.rs Output::present); winit 0.30.13: `WindowEvent::Occluded` on macOS (not Windows), `is_minimized` on Windows / macOS.
- REPRODUCED (GUI runs allowed by Amrit for this wave): glance-proof playing glenc_hap5_1920x1080_chunks4.mov, its window fully covered 10 s by another process's window (docs/data/covered-window-pacing/cover.swift): v0.9.2 60 550 frames/s, 175 % CPU (visible 120 frames/s, 14 %). The spin = frames that get no drawable, back to back; nothing presented while covered (the logged present interval stays at its last value).
- FIX: glance-ui `pace` (crate-private Pacer) for the main window and every output window: a redraw is held to 0.95 of a refresh after the last frame began when the OS says hidden (Occluded / is_minimized), when the last frame got no drawable, or when frames came < 2 ms apart twice in a row; the wait is ControlFlow::WaitUntil in `about_to_wait`, not a sleep; shown again, the next frame draws at once. GlancEhr's bare 2 ms rule (holding a single quick frame) was tried first and dropped: it holds FIFO catch-up frames of a visible window.
- PROVED: 9 unit tests in pace.rs; diag build (prints, not committed): Occluded(true) 19 ms after the cover, (false) 9 ms after; 1 252 frames without a drawable while covered (about 10.5 s x 119 frames/s). A/B back to back under the same load (ab1-new / ab2-old / ab3-new): covered 119.2 / 62 983.9 / 116.5 frames/s, CPU 6.8 / 174.1 / 6.9 %; visible the same within load noise (113.4–119.8 vs 114.1–119.5 frames/s); un-cover longest gap 16.8 / 16.9 / 16.7 ms. docs/data/covered-window-pacing/results.txt.
- Workspace version 0.9.3 (version + lock).
- Gates (docs/data/covered-window-pacing/gates.txt): fmt, clippy, deny, build, path-guard green; tests 468 passed / 3 failed, all three Amrit's test media: clipS_ProRes 4444.mov (871 561 049 bytes) is no longer in the test media folder (folder last changed 21:39:53 local, between this session's last proof run and its gates; no command of this session and nothing in the workspace writes there). Last full run here (kit-dmg wave): 459 / 0. Handoff docs/handoffs/v093-covered-window-pacing.md.
- Pushed 6869ca1 to main through tools/push-main.sh (path-guard green on guard/6869ca1558fb and main); CI on main green: lint, macos, windows (run 44947998261), path-guard (44947998235). Tag v0.9.3 (annotated) on 6869ca1, pushed.
- Note to GlancEhr: ~/claude-handoffs/GlancEhr_2026-11-21_note-from-glance-core-covered-window-pacing.md (drop its local pacing at the next pin bump).
- Verifier (cc-kit:verifier): 27 confirmed, 4 unproven, 3 wrong (presents "every 16 ms" while covered: none were presented; 10 unit tests for 9; "restored" drawn at once: at its due time); corrected in pace.rs's doc, the handoff and here (doc-only commit after the tag); the unproven ones stated in the handoff; load averages added (runs/ab-load.txt).
- Wave closed 2026-11-21. Result: ~/claude-handoffs/glance-core_2026-11-21_covered-window-pacing-result.md.

## NEXT
- Nothing queued for this wave. Amrit: clipS_ProRes 4444.mov is gone from the test media folder, so three test-media tests are red on the home machine (files_swift_parity, routing, swift_parity): put the file back, or have their expectations re-read without it (his media, his call). Next core work: the Windows milestone (ROADMAP §7).

## STATE
- Other sessions push to glance-core main from their own clones (GlancEhr's did, v0.9.0): `git fetch` and compare with origin/main before committing a version or tagging.
- glance-core, DXVKit-Plus and glenc-public are PUBLIC; the old history is only in the private bundles (~/claude-handoffs/backups/<repo>/<repo>_pre-scrub_2026-11-21.bundle). Never push a bundle.
- Gates on the home machine: set GLANCE_TESTFILES to the test media folder, GLANCE_REQUIRE_GPU=1; `tools/path-guard.sh check` is a gate.
- main is protected: push with tools/push-main.sh (rule 11, CLAUDE.md "Pushing"). Tags: git push origin <tag>.
- A changed tools/path-guard.sh or .gitleaks.toml goes to glance-core main first, then tools/path-guard-setup.sh and a commit in every other repo (DXVKit-Plus's and glenc-public's CI compare their copy with glance-core's).
- The kit's temporary folder is KIT_TMP (lib.sh): physical ${TMPDIR:-/tmp}. A record of the app's id under it whose path is gone is the kit's own (ls-cleanup.sh).

## OPEN

## IDEAS
- Windows: Media Foundation reader / writer behind glance-encode's platform module (with DXVKit Plus's Windows backend).
- Kit: dmg.sh's mount point in its own folder, away from the staging copy (runs E and I of kit-dmg-record-fix register nothing): removes the cause; the trap's unregister then has nothing to do. Or an unregister straight after the guard's detach, so the record does not live through the notarise wait.
- Kit: macOS 27 prints a deprecation warning for `hdiutil create / attach / detach` ("use diskutil image ..."); move when it stops working or earlier.
