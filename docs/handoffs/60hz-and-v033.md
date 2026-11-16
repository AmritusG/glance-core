project:    glance-core
pass:       60hz-and-v033
STATUS:     COMPLETE
parent:     6bdc3d9
result:     6816383 (tag v0.3.3) + the handoff commit on main after it
session:    <session>
written_at: 2026-11-16T21:21Z

# glance-core 60 Hz run and v0.3.3 (Show All): handoff

Brief: `~/claude-handoffs/glance-core_2026-11-16_60hz-run-and-v033-show-all_brief.md` (part B:
`glance-core_2026-11-16_app-menu-show-all_brief-r2.md`). Machine: <host>, Apple M5 Max, macOS 27.0.1. Tag **v0.3.3** → 6816383.

## Amrit's answers
- 2026-11-16 21:37 (Cowork, via the brief): "Apple report has been sent. 60 Hz run can be done now also."
- 2026-11-16 (this session), which app may be hidden for the one-app-hidden check: **"Hide Terminal"**.

## PART A: real 60 Hz
- **Display:** Panasonic PT-RZ570 projector, CoreGraphics display 3 (vendor 13481, model 53534; winit "Monitor #53534"),
  1920 x 1200 points = pixels (scale 1), **60.00 Hz** as macOS reports it, to the right of the built-in display. Its
  settings were not touched. Evidence: `docs/data/v033-60hz/displays.txt`, `system-profiler-displays.txt`.
- **Method:** C2's 13 runs (`tools/c2-measure.sh`), the same clips, 15 s after 1 s warm-up, two passes, release build,
  **pace 1** on the projector (no simulation). The proof had no way to choose a display and macOS opens windows on the
  main screen, so `glance-proof --display <name>` was added. It moves the window to the centre of that display on the
  first frame, inside the warm-up, and every JSON records `"display": "Monitor #53534"` and `"display_refresh_hz": 60.0`
  (26 of 26). The runner now takes paces and a display, and logs one CSV row per refresh.
- **Load:** the runs started after another session's `tl-play` run (TimeLinEhr) had ended and 30 s passed with no
  rustc, cargo or tl-play. During the runs (84 samples every 5 s, `load.txt`), Arena was open (25–46 % CPU, untouched)
  and two long-running Python processes reached 880 %. The other session built again (rustc up to 624 %,
  21:50–21:54) and restarted `tl-play` (21:53, 21:55–21:57). Also running: another project's app `tkapp_check` (up to 170 %) and
  `ui_crawl`, Time Machine (`backupd` up to 122 %), Spotlight and Chrome. There was load beyond Arena in nearly every
  run. **Not an idle machine.** The measuring started none of it.
- **Result: 0 skipped in all 26 runs, 11,139 frames shown.** 1080p and 2160p, HAP and DXV, with and without sound
  (audio clock on PCM and AAC). The picture equals the CPU decode (max 0) in every run. Presents: p50 16.66–16.68 ms,
  p95 17.08–22.16 ms, max 27.56 ms; no interval reached two refreshes. Per run: `docs/05-C2-RESULTS.md` §1a,
  `docs/data/v033-60hz/summary.txt`.
- **Against C2's simulation** (pace 2 on the 120 Hz display): 4 of 26 runs there skipped 1–3 frames, all when two late
  presents in a row came between the frames the clock asked for (C2's explanation; those pass-1 runs have no per-refresh
  log). On the real display the clock is asked at every
  refresh and the presents came steadily, so no skips.
- **Parity:** core item "No clip frame is skipped at 1x on a 60 Hz or 120 Hz display …" ticked (macOS; Windows is C4).
  Roadmap C2 row: 5 of 6.

## PART B: v0.3.3, Show All
- **RED** (bundled sample = scratch copy of `/Applications/Glance Sample.app` with the v0.3.2-code binary, ad-hoc signed,
  against Swift Glance a9dc601 built as `com.amritus.glance.swiftref` from `.ref/glance`; both started with `open -a`,
  `tiles_dxt5.mov` copied to the scratchpad outside ~/Documents; no app hidden): one difference, **Show All: Swift
  off, ours on** (`docs/data/v033/menus-red-diff.txt`).
- **Fix:** `glance_platform::finish_app_menus` turns AppKit validation (`autoenablesItems`) on for the app menu, which
  it finds by its `unhideAllApplications:` item. NSApp then validates Hide Others and Show All. About, "Set … as
  Default for .mov Files…" and Quit stay on: muda's items are their own target and take their action, and nothing in
  the app menu is disabled by the app.
- **GREEN, no app hidden:** run 2, with each app frontmost just before its dump (logged): **equal but for the app name**
  (`menus-green2-diff.txt` empty, Show All off in both). A sample-only A/B with 5 + 5 alternating launches gave
  **Show All off 5/5 (v0.3.3) against on 5/5 (v0.3.2)** (`docs/data/v033/ab/`).
- **Found on the way, not caused by this change:** in GREEN runs 1 and 3, and in A/B runs 3–5 of *both* builds, the
  sample's Window-menu window items (Minimize, Zoom, Fill, Center, Move & Resize, Full Screen Tile) read **off**, and
  Full Screen Tile's sub-items, Move to PT-RZ570 and Move All to PT-RZ570 are **missing**. In GREEN 1 the Swift dump
  also lacked "Quit and Keep Windows". It happens in v0.3.2 too (A/B red 3–5), so it is not this change. **Cause not
  established:** in A/B runs 3–5 another app (Finder, Claude) was frontmost just before most of the dumps, but in GREEN
  3 and A/B red 3 the sample was frontmost just before its dump and the items still read off. Another session's UI
  automation (`ui_crawl`) ran on the machine earlier. Read 8 s after opening, the window was main and focused and the
  items were on in both builds (`winprobe.sh`, output in `session-evidence.txt`). Open for glance-rs's menus run: if
  it shows this, re-run and compare.
- **One app hidden** (Amrit: "Hide Terminal"): Terminal was hidden through System Events and the dumps re-run:
  **Show All on in both, equal but for the app name** (`menus-hidden-terminal-diff.txt` empty). Terminal was shown
  again straight after (`visible: true`).
- **Clean-up:** the three scratch bundles were unregistered from LaunchServices; only `/Applications/Glance Sample.app`
  remains, as before (`lsregister-after.txt`). No keys or clicks were sent. Nothing touched `com.amritus.glance`,
  `/Applications/Glance.app` or Arena.
- **Gates:** fmt, clippy `-D warnings`, `GLANCE_REQUIRE_GPU=1` tests 71 passed 0 failed, deny, build (local, on the
  working tree that became 6816383; the first fmt check failed on one line, which `cargo fmt` fixed before this run).
  CI run 43957237691 green on 6816383. Annotated tag v0.3.3 pushed.
- **Note for glance-rs:** `~/claude-handoffs/glance-rs_2026-11-16_note-from-glance-core-v033-r2.md`. It supersedes the
  first note, which had the Window-items and load mistakes. Bump the pin, fix
  CLAUDE.md's stale `v0.3.1` line, re-run the menus run and tick the menus item from it, record "Apple report sent
  by Amrit 2026-11-16" for item 14, and use the 60 Hz result for item 17.

## Deviations
- New proof option and two glance-ui `Host` methods (`move_to_display`, `display_name`): the brief asked for the window
  on the projector, and nothing could put it there without code or a display-settings change.
- The GREEN sample binary was built before `cargo fmt` (one line of glance-proof re-wrapped) and before the version bump
  to 0.3.3. Its menu code is identical to the tagged commit.
- `menus-red-run.txt` and `menus-green1-run.txt` predate the frontmost logging (added to `menus.sh` after GREEN 1).
- Output that exists only from the session (Amrit's answer, the A/B frontmost reads, the window probe, binary build
  times): `docs/data/v033/session-evidence.txt`.
- After the tag, on main: the glance-proof doc comment for `--display` now says winit's names ("Monitor #<model
  number>"), and the load text in `05-C2-RESULTS.md` §1a and the parity line were corrected after the verifier read
  `load.txt`.
- The machine was not idle during Part A (see Load). The result is 0 skipped even so.

## Commits
- 6816383 (tag v0.3.3): the fix, the 60 Hz run, the docs and the data.
- The handoff commit follows on main: corrections, session evidence, the handoff, the closed checkpoint.

## Verifier
cc-kit:verifier on the CHECKPOINT and the handoff draft: 32 confirmed, 13 unproven, 5 wrong. All five were corrected:
the load during the runs, "Move to PT-RZ570" (missing, not off), the cause of the Window items (not established), the
`--display` doc comment, and the checkpoint and handoff not yet closed. Unproven ones: the session-only output is now
saved in `session-evidence.txt`. These stay unproven by any file: display settings untouched, no keys or clicks sent,
Arena untouched. No configuration call, key or click tool was used, and the scripts in `docs/data/v033/` contain none.

## RECEIPTS
```
$ git log -1 --format="%H %d %s" 6816383
68163836540d74e5a663a7e3d53757f1b6b2b910  (HEAD -> main, tag: v0.3.3, origin/main, origin/HEAD) v0.3.3: app menu validated by AppKit (Show All as Swift's); 60 Hz no-skip run on a real display (0 of 26 skipped)
$ git ls-remote origin main refs/tags/v0.3.3 "refs/tags/v0.3.3^{}"
68163836540d74e5a663a7e3d53757f1b6b2b910	refs/heads/main
dc63fe8c4bafe07eecd2ef1534efdf3780f1f07a	refs/tags/v0.3.3
68163836540d74e5a663a7e3d53757f1b6b2b910	refs/tags/v0.3.3^{}
$ gh run view 43957237691 --json headSha,conclusion,jobs
68163836540d74e5a663a7e3d53757f1b6b2b910 success
  windows: success
  macos: success
  lint: success
$ cat docs/data/v033/gates.txt
== 2026-11-16 22:06:25 6bdc3d9+working tree
fmt: ok
    Finished `release` profile [optimized + debuginfo] target(s) in 0.31s
clippy exit 0
tests (GLANCE_REQUIRE_GPU=1): 71 passed, 0 failed
advisories ok, bans ok, licenses ok, sources ok
    Finished `release` profile [optimized + debuginfo] target(s) in 1.01s
$ grep '^runs' docs/data/v033-60hz/summary.txt
runs 26, skipped total 0, shown total 11139
$ grep -h "\"display\|display_refresh" docs/data/v033-60hz/pass*/*.json | sort | uniq -c
  26   "display_refresh_hz": 60.0,
  26   "display": "Monitor #53534",
$ wc -c docs/data/v033/menus-*-diff.txt
    2393 docs/data/v033/menus-green1-diff.txt
       0 docs/data/v033/menus-green2-diff.txt
    2341 docs/data/v033/menus-green3-diff.txt
       0 docs/data/v033/menus-hidden-terminal-diff.txt
      71 docs/data/v033/menus-red-diff.txt
    4805 total
$ cat docs/data/v033/menus-red-diff.txt
9c9
< Glance | Show All |  |  | off
---
> Glance | Show All |  |  | on
$ grep -h "Show All" docs/data/v033/menus-{red,green2,hidden-terminal}-{sample,swift}.txt
red-sample: Glance Sample | Show All |  |  | on
red-swift: Glance | Show All |  |  | off
green2-sample: Glance Sample | Show All |  |  | off
green2-swift: Glance | Show All |  |  | off
hidden-terminal-sample: Glance Sample | Show All |  |  | on
hidden-terminal-swift: Glance | Show All |  |  | on
$ grep hidden docs/data/v033/menus-hidden-terminal-run.txt
hidden apps (before): [Terminal]; frontmost: [Claude]
hidden apps (sample up): [Terminal]; frontmost: [glance-sample]
hidden apps (swiftref up): [Terminal]; frontmost: [Glance]
hidden apps (after): [Terminal]; frontmost: [Claude]
Terminal visible after: true
$ ls -la docs/data/v033 docs/data/v033-60hz; ls docs/data/v033-60hz/pass1 docs/data/v033-60hz/pass2 | grep -c json
docs/data/v033:
total 184
drwxr-xr-x  28 <user>  staff   896 16 Nov 22:09 .
drwxr-xr-x  22 <user>  staff   704 16 Nov 21:59 ..
drwxr-xr-x  12 <user>  staff   384 16 Nov 22:05 ab
-rw-r--r--   1 <user>  staff   312 16 Nov 22:06 gates.txt
-rw-r--r--   1 <user>  staff    70 16 Nov 22:09 lsregister-after.txt
-rw-r--r--   1 <user>  staff  2393 16 Nov 22:00 menus-green1-diff.txt
-rw-r--r--   1 <user>  staff  2608 16 Nov 22:00 menus-green1-run.txt
-rw-r--r--   1 <user>  staff  3384 16 Nov 22:00 menus-green1-sample.txt
-rw-r--r--   1 <user>  staff  3411 16 Nov 22:00 menus-green1-swift.txt
-rw-r--r--   1 <user>  staff     0 16 Nov 22:01 menus-green2-diff.txt
-rw-r--r--   1 <user>  staff   306 16 Nov 22:01 menus-green2-run.txt
-rw-r--r--   1 <user>  staff  3574 16 Nov 22:01 menus-green2-sample.txt
-rw-r--r--   1 <user>  staff  3455 16 Nov 22:01 menus-green2-swift.txt
-rw-r--r--   1 <user>  staff  2341 16 Nov 22:02 menus-green3-diff.txt
-rw-r--r--   1 <user>  staff  2649 16 Nov 22:02 menus-green3-run.txt
-rw-r--r--   1 <user>  staff  3384 16 Nov 22:01 menus-green3-sample.txt
-rw-r--r--   1 <user>  staff  3455 16 Nov 22:01 menus-green3-swift.txt
-rw-r--r--   1 <user>  staff     0 16 Nov 22:09 menus-hidden-terminal-diff.txt
-rw-r--r--   1 <user>  staff   365 16 Nov 22:09 menus-hidden-terminal-run.txt
-rw-r--r--   1 <user>  staff  3573 16 Nov 22:09 menus-hidden-terminal-sample.txt
-rw-r--r--   1 <user>  staff  3454 16 Nov 22:09 menus-hidden-terminal-swift.txt
-rw-r--r--   1 <user>  staff    71 16 Nov 22:00 menus-red-diff.txt
-rw-r--r--   1 <user>  staff   286 16 Nov 22:00 menus-red-run.txt
-rw-r--r--   1 <user>  staff  3573 16 Nov 22:00 menus-red-sample.txt
-rw-r--r--   1 <user>  staff  3455 16 Nov 22:00 menus-red-swift.txt
-rwxr-xr-x   1 <user>  staff  1873 16 Nov 22:01 menus.sh
-rw-r--r--   1 <user>  staff  1643 16 Nov 21:59 swift-ref.sh
-rwxr-xr-x   1 <user>  staff  1158 16 Nov 22:02 winprobe.sh

docs/data/v033-60hz:
total 112
drwxr-xr-x  10 <user>  staff    320 16 Nov 22:09 .
drwxr-xr-x  22 <user>  staff    704 16 Nov 21:59 ..
-rw-r--r--   1 <user>  staff    444 16 Nov 21:41 disp.swift
-rw-r--r--   1 <user>  staff    199 16 Nov 21:41 displays.txt
-rw-r--r--   1 <user>  staff  35718 16 Nov 21:57 load.txt
drwxr-xr-x  29 <user>  staff    928 16 Nov 21:54 pass1
drwxr-xr-x  29 <user>  staff    928 16 Nov 21:57 pass2
-rw-r--r--   1 <user>  staff   2071 16 Nov 21:50 sum60.py
-rw-r--r--   1 <user>  staff   3735 16 Nov 21:57 summary.txt
-rw-r--r--   1 <user>  staff    714 16 Nov 21:50 system-profiler-displays.txt
26
$ wc -c docs/data/v033/session-evidence.txt
    3371 docs/data/v033/session-evidence.txt
$ cargo fmt --all --check (after the doc-comment fix)
fmt: ok
$ cargo clippy -p glance-proof --release -- -D warnings
    Finished `release` profile [optimized + debuginfo] target(s) in 0.30s
```
END OF HANDOFF 60hz-and-v033 sha256:25e43c46 13646
