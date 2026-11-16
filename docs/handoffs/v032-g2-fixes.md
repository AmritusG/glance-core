project:    glance-core
pass:       v032-g2-fixes
STATUS:     BLOCKED
parent:     5d529ff
result:     8c90e4d (no tag: v0.3.2 waits for Amrit's decision on the drop crash)
session:    glance-core Claude Code, 2026-11-16 (<session>)
written_at: 2026-11-16T17:30Z

# glance-core v0.3.2: menus as Swift's, Esc on alerts, the parity line done; the drop crash is macOS's (handoff)

Brief: `~/claude-handoffs/glance-core_2026-11-16_v032-g2-fixes_brief.md` with
`…_g2-findings_brief-r2.md`. Machine: <host> (Apple M5 Max, macOS 27.0.1). Raw runs:
`docs/data/v032/` (probe sources and the safe drop tools in `probes/`). Clips: the repo's own fixtures
`tests/fixtures/tiles_dxt1.mov`, `tiles_hap1.mov`, `tiles_dxt5.mov`; no media of Amrit's was used.

**Decision for Amrit (one line):** the drop crash is a macOS bug in its drag code, not in winit or the
core. Do I tag v0.3.2 now without a crash fix (and draft a report to Apple), or hold the tag?

## Amrit's answers this wave
- "Free now" (the machine for the on-screen runs) and "Yes, off for the run" (the crash dialog). macOS 27
  ignores `defaults write com.apple.CrashReporter DialogType none`. The key was absent before and was
  removed again after.
- "finish the run, be consistent and dont make mistakes, or at the very least LEARN from mistakes."

## Incident (read first)
The first diagnosis runs (until about 16:05) posted mouse drags and drops with posted HID events and
didn't check what was under the pointer. Over about an hour, drops went into Amrit's browsers and onto
macOS's "quit unexpectedly" dialogs. Amrit lost unsaved browser work. A Safari crash report from 16:03:14
has the same heap-corruption signature as this bug (`safari-crash-summary.txt`).

From 16:10 on, every drop went through `probes/safe-drag.swift` and `safe-drop.sh`:
- the tool drops only on the test app's own topmost window, otherwise it carries the drag home and aborts;
- the run stops at the first problem;
- after a crash it presses Ignore on that app's crash dialog only.

Those checks refused unsafe drops three times (Dock overlay, Notification Centre, a crash dialog) and
nothing was dropped elsewhere after that. The rule is saved as a standing memory for this project.

## READ (sources)

| Fact | Source |
|---|---|
| winit 0.30.13 declares `draggingEntered:` / `prepareForDragOperation:` / `performDragOperation:` returning `bool`, registers the window for `NSFilenamesPboardType` | `winit-0.30.13/src/platform_impl/macos/window_delegate.rs:367-428, 666-669` |
| winit master returns `NSDragOperation` (rewrite #4571, merged 2026-07-16). The earlier non-file-content fix (82eab465, 2025-11-05) was backported as ab4c6bfc and is in 0.30.13. No 0.30.x release returns `NSDragOperation` | `gh api repos/rust-windowing/winit` (pull 4571, commits on `winit-appkit/src/window_delegate.rs`, the v0.30.x branch file); winit 0.30.13 changelog |
| `draggingEntered:` returns `NSDragOperation` | Apple, NSDraggingDestination `draggingEntered(_:)` |
| A write to a W register zeroes the upper 32 bits of the X register; winit's compiled `dragging_entered` returns via `cset w0, ne; ret` (x0 = 0 or 1) | `objdump -d target/release/glance-sample` at the symbol; Arm A64 register rule |
| muda 0.21.2 sets `autoenablesItems(false)` on every submenu; predefined items have a nil target; `close_window` is titled "Close Window", `fullscreen` "Toggle Full Screen"; `delete` is macOS-only | `muda-0.21.2/src/platform_impl/macos/mod.rs:127, 233, 675, 740-760, 945-980`, `src/items/predefined.rs:105-111, 430-449` |
| Swift Glance a9dc601: SwiftUI `Window` scene, no own fullscreen item ("AppKit auto-adds View → Enter Full Screen"); drop target is the view (`draggingEntered` → `.copy`, `readObjects(forClasses: [NSURL])`) | `.ref/glance/Sources/Glance/GlanceApp.swift:36-125`, `VideoHostingView.swift:274-285` |
| The running Swift reference: Window menu = Minimize, Zoom, separator, separator, Bring All to Front; window `tabbingMode` 2 (disallowed), `isExcludedFromWindowsMenu` 1, `collectionBehavior` 0x10080, `allowsAutomaticWindowTabbing` still 1; both apps' Remove Window from Set is `_removeWindowFromStageManagerSet:` | lldb on the scratch Swift build (re-signed copy) and on the sample |
| wgpu-hal reads `presentsWithTransaction` from the layer every frame and then presents in the transaction | `wgpu-hal-30.0.2/src/metal/surface.rs:400`, `metal/mod.rs:802-823` |
| NSAlert gives Esc to a button titled "Cancel" (or "Don't Save") only; other buttons need a key equivalent | Apple, `NSAlert addButton(withTitle:)` |
| raw-window-metal 1.1.0 (MIT OR Apache-2.0) uses the view's layer if it is a CAMetalLayer, else inserts one as a sublayer | `raw-window-metal-1.1.0/src/lib.rs` (`from_ns_view`) |

## DIAGNOSE 1: the drop crash

Each try opens `tiles_dxt1.mov`, then drags `tiles_hap1.mov` onto the video area. The source quits at once
after the drop (`DRAG_LINGER=0`).

| Build | Crashed | Harness | Run file |
|---|---|---|---|
| **RED**, v0.3.1 sample | 4/10 | unsafe | `red-drop.txt` |
| v0.3.1 sample, NSZombieEnabled | 0/6 | unsafe | `zombie-drop.txt` |
| v0.3.1 sample, guard malloc (load not confirmed) | 0/4 | unsafe | `gmalloc-drop.txt` |
| v0.3.1 sample, MallocPreScribble + MallocScribble | 0/6 | unsafe | `scribble-drop.txt` |
| sample with winit patched to return NSDragOperation (falsifying run) | **4/20** | unsafe | `falsify-nsdragoperation.txt` |
| plain winit 0.30.13 window, waiting | 0/10 | unsafe | `winit-min-wait.txt` |
| plain winit, redrawing continuously | 0/10 | unsafe | `winit-min-poll.txt` |
| plain winit, 4000 × 16–512 B allocations a frame | 0/20 | unsafe | `winit-min-churn.txt` |
| `glance_ui::run_shell`, empty app | 1/10 | unsafe | `probe-shell.txt` |
| winit + wgpu surface, Fifo, continuous | 2/20 | unsafe | `probe-wgpu.txt` |
| winit + wgpu surface, drawn on demand | 0/20 | safe | `probe-wgpu-idle.txt` |
| winit + wgpu surface, Immediate | 7/20 | safe | `probe-wgpu-immediate.txt` |
| winit + wgpu surface, Immediate + `presentsWithTransaction` | 9/20 | safe | `probe-wgpu-immediate-txn.txt` |
| plain winit, 4000 × **512–1024 B** a frame (no Metal, no egui, no core code) | **4/20** | safe | `winit-min-churn-512-1024.txt` |
| same, winit reading file URLs as Swift does | 2/20 | safe | `winit-min-churn-fileurl.txt` |

A crash is "app GONE". Some reports land after the try counts them, so a row's "new crash reports" can
read 0 on a crash. In the unsafe rows, "title after drop" names the dropped clip on every try where the
app stayed alive, except three:
- `zombie-drop` try 1 and `falsify-nsdragoperation` try 10: the title still shows `tiles_dxt1.mov`, so no file was opened;
- `winit-min-churn` try 17: no title read.

Those three drops may have landed elsewhere. The safe runs refused to drop anywhere but the test app
(refusals quoted in `session-evidence.txt`).

**The fault** (lldb, `lldb-crash.txt`; disassembly excerpts in `session-evidence.txt`): `AESizeOfFlattenedDesc` reads `[x0+0x10]`, where x0 is the
data handle of the AEDesc at `DragInfo+0x74` (`BuildIPCPackage`). The handle holds a value just under
2^32 (0xFFFF_FFF9), not a pointer. The call chain is `CoreDragEndDragging ← CoreDragTerminate ←
RemotePortInvalidationCallback`, so it runs when the dead source's port is noticed.
`RemotePortInvalidationCallback` takes the drag ref from its context and `CoreDragTerminate` looks it up
(`GetDragInfo`) before ending it (disassembly). The other crash kind traps on a corrupted xzone free list
in whatever code allocates next (egui, wgpu, AppKit's layout engine, Safari's CloudTabStore). That means
something wrote to freed memory.

**What is proven:** the crash needs no code of ours and no Metal. Plain winit crashes at the same rate
as the sample once the heap reuses 512–1024-byte blocks, and never without that. winit's `bool` return
is not the cause (reading, and 4/20 with it fixed). How the destination reads the drop doesn't matter
(2/20 reading file URLs). Present rate matters only through allocation: on demand 0/20, Fifo 2/20,
Immediate 7/20.

**Not proven:** which HIServices function frees or overwrites the record. Every allocator-debugging mode
made the crash vanish (0/16 in total), and lldb's trace mode would not resume the app. Swift Glance's 0/16
in G2 is consistent with a main thread that rarely reuses that size class; I did not measure it.

**Contradiction with the brief:** the brief expects a fix in the core or in winit. No fix at the cause is
possible from here, because the cause is Apple's. I stopped the crash item, as the brief instructs.
Measured options for Amrit, none built:
1. Tag v0.3.2 without a crash fix and send Apple a Feedback Assistant report. The plain-winit + churn
   repro needs only `probes/`. Build `winit-min-main.rs.txt` as a crate with `winit = "=0.30.13"` (binary
   named `glance-sample`) and compile `xcrun swiftc -O -o safe-drag safe-drag.swift` beside `safe-drop.sh`,
   `window-id.swift` and `dismiss-crash.sh`. Then run
   `POLL=1 CHURN=1 LO=512 HI=1024 CONTINUE_AFTER_CRASH=1 ./safe-drop.sh <bin> <clips> 20 <out>`, only on a
   free machine.
2. Reduce exposure without fixing it: no tested change reached 0/20 while presenting continuously.
   Drawing on demand reached 0/20 but would stop video playback, so it isn't usable as is.

## DIAGNOSE 2: menus (RED)
Accessibility dump (glance-rs `tools/menu-dump.swift` unchanged: `probes/copied-tools-sha256.txt`)
of the installed v0.3.1 sample against the Swift reference, both with `tiles_dxt5.mov` open
(`menus-red-v031.txt`, `menus-swift.txt`, `menus-red-diff.txt`). All six of the brief's differences
reproduce, plus one more: Swift's Window menu has a second separator before Bring All to Front.
`menus-red-diff.txt` was regenerated from the committed dumps.

The Swift reference must be opened with `open -a`, which is how glance-rs ran it. A path in argv opens
nothing, and the clip has to sit outside ~/Documents. In ~/Documents the build asks for folder access, and
that prompt was answered "Don't Allow" without granting anything.

## WHAT SHIPPED (8c90e4d, version 0.3.2)
1. **Menus** (`glance_ui::menu`, `glance_platform::{set_single_window, finish_app_menus}`):
   - File ▸ "Close".
   - muda's Toggle Full Screen and its separator removed; AppKit adds its own Enter / Exit Full Screen ⌃⌘F, as for Swift.
   - The window takes Swift's values, read from the running app: `tabbingMode` disallowed (no Show Tab
     Bar / Show All Tabs), excluded from the Window menu, `collectionBehavior` FullScreenPrimary | Primary.
   - The Edit and Window menus are validated by AppKit (`autoenablesItems`). With nothing to edit the Edit
     items are off, and Remove Window from Set is off, as in Swift.
   - macOS only: Edit ▸ Delete, and the second Window-menu separator.
   - Help ▸ "<app> Help" ⌘? sends `showHelp:`.
   - A first attempt switched tabbing off for the whole app (`allowsAutomaticWindowTabbing = false`). Swift
     doesn't do that, so it was replaced by the per-window setting.
2. **Esc on two-button alerts** (`glance_platform::alert`, macOS): the second button gets Esc as its key
   equivalent. Return keeps the first. Windows already answered Esc with Cancel (MB_OKCANCEL → index 1).
3. **Parity:** `docs/02-PARITY.md` Glance Esc line changed as settled in the brief, left unticked for glance-rs.
4. README "What 0.3.2 changes", which says plainly that the crash isn't fixed.
5. Removed: the temporary probes (`crates/glance-ui/examples/`, sources in `docs/data/v032/probes/`) and a
   `present_with_transaction` helper that did not help (9/20).

## BUILD + TEST
- **Menus (GREEN):** I dumped the sample in a scratch copy of the installed bundle, with the new binary and
  an ad-hoc signature, so the app name and Quit and Keep Windows come out as in a bundled app. The dump
  equals Swift's but for the app name: `menus-green-diff.txt` is empty. The Glance ▸ Show All line
  happened to match this time.
- **Help:** both apps show an alert, "Help isn't available for Glance" / "… for Glance Sample.", with OK
  (`help-swift.txt`, `help-green.txt`). Ours was closed with the app; nothing was left on screen.
- **Esc:** `alert_probe` shows `glance_platform::alert` with "Set as Default" / "Not Now" inside winit's
  event loop, from a scratch copy of the sample bundle, and receives one key, sent only after checking
  that the probe is frontmost.
  - v0.3.1: Esc leaves the alert up (`red-alert-esc.txt`).
  - v0.3.2: Esc → `Ok(1)` (Not Now), Return → `Ok(0)` (`green-alert-esc.txt`, `green-alert-return.txt`).
  - One Return try ended with `Ok(0)` before any key was sent, because a key reached it from outside the
    run. That try was discarded. Left without keys, the alert stays up (8 s check).
- **Session-only evidence:** the Swift lldb values, the disassembly, the safety refusals, the discarded
  Return try, the Documents-access prompt and the Help close are quoted in `session-evidence.txt`.
- **Drop crash:** not fixed, see above. The brief's ≥ 20 tries with 0 crashes are not met.
- **Gates:** green locally and on CI for 8c90e4d (RECEIPTS).
- **LaunchServices:** one record left, `/Applications/Glance Sample.app` (v0.3.1). This wave's scratch bundles
  were unregistered. So was a stale `com.amritus.glancecore.swiftref` record from the 2026-11-15 session's
  scratchpad, which the verifier found.

## For glance-rs
Note: `~/claude-handoffs/glance-rs_2026-11-16_note-from-glance-core-v032.md`. Mailbox copy of this handoff:
`~/claude-handoffs/glance-core_2026-11-16_v032-g2-fixes-result_BLOCKED.md`. CLAUDE.md's `-result` name, plus the
handoff skill's status suffix for a BLOCKED pass. No tag yet. When v0.3.2 is
tagged, bump the pin, re-run the menus run (the system items now match), and keep
`tools/g2-drop-crash.sh` as the crash repro. It will still crash, because the cause is macOS's. glance-rs's
`drag-file.swift` should get the target check of `probes/safe-drag.swift` before anyone runs it again.

## RECEIPTS
```
$ git log -1 --format='%H %s' 8c90e4d   (pushed: main...origin/main, no ahead/behind)
8c90e4dd888e38b0a8de4f240181afc44ae174f2 v0.3.2: system menus as Swift Glance's, Esc = Not Now on two-button alerts, parity Esc line; drop crash diagnosed (macOS CoreDrag), not fixed
$ git tag --list v0.3.2
(none: the tag waits for Amrit)
$ gh run view 43902765934   (CI on 8c90e4d)
windows: success
lint: success
macos: success
conclusion: success  https://github.com/AmritusG/glance-core/actions/runs/43902765934
$ cargo fmt --all --check
(no output, exit 0)
$ cargo clippy --workspace --all-targets --release -- -D warnings
    Finished `release` profile [optimized + debuginfo] target(s) in 0.66s
$ GLANCE_REQUIRE_GPU=1 cargo test --workspace --release
25 test binaries: 71 passed; 0 failed
$ cargo deny check
advisories ok, bans ok, licenses ok, sources ok
$ cargo build --release
    Finished `release` profile [optimized + debuginfo] target(s) in 1.66s
$ ls docs/data/v032 | wc -l ; du -sh docs/data/v032
30
188K	docs/data/v032
$ wc -c docs/data/v032/menus-green-diff.txt
0 docs/data/v032/menus-green-diff.txt
$ cat docs/data/v032/green-alert-esc.txt docs/data/v032/green-alert-return.txt docs/data/v032/red-alert-esc.txt
key 53 -> answer Ok(1)
key 36 -> answer Ok(0)
probe still showing the alert after key 53: killed
key 53 ->
$ lsregister -dump | grep -E 'Glance (Sample|SwiftRef)|glancecore'
path:                       /Applications/Glance Sample.app (0x128a4)
$ defaults read com.apple.CrashReporter DialogType
Error: Could not find key 'DialogType' in domain 'com.apple.CrashReporter'.
```
END OF HANDOFF v032-g2-fixes sha256:251ea8fb 15348
