# v0.9.3 — a covered or minimised window no longer spins the frame loop

Brief: GlancEhr's all-functions-and-transitions pass found that a covered window made its frame loop spin and fixed it
in its own `frame`; this moves the fix into glance-ui's shell so the whole family gets it (brief
`glance-core_2026-11-21_from-glancehr_covered-window-pacing`).

## What shipped
- **glance-ui `pace`** (crate-private, no API change): the main window and every output window keep a `Pacer`. A
  redraw is held until 0.95 of a display refresh after the last frame began when
  1. the OS says the window is hidden: winit's `Occluded` (macOS), `is_minimized` (Windows, macOS);
  2. the last frame got no drawable (`get_current_texture` gave no texture): the spin measured here, whatever the OS
     says;
  3. frames came less than 2 ms apart twice in a row (no display paces that; 240 Hz is 4.2 ms): the last net for a
     hidden window the OS does not report that still gets drawables at once (at most two frames a refresh).
  The wait is the event loop's (`ControlFlow::WaitUntil`, set in `about_to_wait` to the earliest held redraw of all
  windows), not a sleep: menus, OS events and the other windows carry on. Shown again (`Occluded(false)`, restored),
  the held redraw is asked for and drawn at once.
- The rate while hidden is one display refresh, as visible, so an app that runs its engine in `frame` (GlancEhr's
  show) is never slowed by a covered window. Its local copy (sleep in `frame` when a frame comes within 2 ms) can go at
  its next pin bump; with both, frames of a covered window come a refresh apart and GlancEhr's check no longer fires
  there (while visible it still holds the odd FIFO catch-up frame, as it does today).
- Why not GlancEhr's rule as it is: it holds a single quick frame. A visible window's FIFO queue hands out a drawable
  at once now and then after a late frame (1 to 14 times in 8 s in these runs, never twice in a row); holding that one
  would present it a refresh late. It was tried first ("after" in results.txt) and replaced by the three rules above.

## Validated (release builds, the home machine, 120 Hz display)
Method: glance-proof plays glenc_hap5_1920x1080_chunks4.mov (test media folder, no audio) for 30 s; from 10 s to 20 s
after its window appears another process's opaque window covers it fully (`docs/data/covered-window-pacing/cover.swift`,
no input events); CPU time sampled every 0.5 s; 1 s left out at each edge. All data:
`docs/data/covered-window-pacing/results.txt`, `runs/`.

| run | covered frames/s | covered CPU % | visible frames/s (before / after cover) | longest gap at un-cover |
|---|---|---|---|---|
| v0.9.2 (before) | 60 550.8 | 174.6 | 120.0 / 120.0 | 16.1 ms |
| v0.9.3 ab1 | 119.2 | 6.8 | 113.4 / 119.0 | 16.8 ms |
| v0.9.2 ab2 | 62 983.9 | 174.1 | 114.1 / 119.5 | 16.9 ms |
| v0.9.3 ab3 | 116.5 | 6.9 | 119.8 / 118.4 | 16.7 ms |

- ab1 / ab2 / ab3 ran back to back under the same load (load average 5.3 to 6.6: Blender and Arena were running), so
  visible-cadence differences between them are load, not the change: missed refreshes (gaps over 12 ms) in the first
  visible phase 49 / 46 / 2 (gaps.py).
- Diagnostic build (two prints, not committed; `runs/diag-*.txt`): `Occluded(true)` 19 ms after the cover,
  `Occluded(false)` 9 ms after it was taken away; 1 252 frames without a drawable while covered (every covered frame).
- Unit tests `crates/glance-ui/src/pace.rs` (10): hidden → one frame a refresh, not as fast as the loop turns; no
  drawable → paced whatever the OS says; drawables at once with no OS signal → at most two a refresh; a display-paced
  visible window and one FIFO catch-up frame are never held; shown again → drawn at once; refresh 24–240 Hz, 60 when
  unknown.
- Not measured: Windows (the Windows milestone; the rules are the same code, `is_minimized` and the no-drawable rule
  are what fire there) and an occluded output window (same `Pacer`, its own screen's refresh).
- Gates (`docs/data/covered-window-pacing/gates.txt`): fmt, clippy -D warnings, deny, build, path-guard green; tests
  468 passed, 3 failed, all three Amrit's test media: `clipS_ProRes 4444.mov` (871 561 049 bytes, 10 s 1080p ProRes
  4444) is no longer in the test media folder (`files_swift_parity` finds 182 of the oracle's 183,
  `routing.rs` counts (174, 7) for (174, 6), `swift_parity.rs` reads it as missing). Nothing in the workspace writes
  there; the folder was last changed at 21:39:53 local time on 2026-11-21, outside this session. CI has no test media
  and is unaffected.

## Open
- Amrit: `clipS_ProRes 4444.mov` left the test media folder; put it back, or the three tests need their expectations
  re-read from the Swift oracle without it.
- GlancEhr: drop its local pacing at the next pin bump (note sent).
