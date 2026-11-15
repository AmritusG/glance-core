project:    glance-core
pass:       v031-g1-fixes
STATUS:     COMPLETE
parent:     ae8930d
result:     a56690a 435920b d96e6b7 + the closing commit, tagged v0.3.1
session:    glance-core Claude Code, 2026-11-15
written_at: @WRITTEN@

# glance-core v0.3.1: the four things Glance G1 found (handoff)

Brief: `~/claude-handoffs/glance-core_2026-11-15_v031-g1-fixes_brief.md`, with the two it names
(`…_g1-findings_brief.md`, `…_folder-listing_brief.md`). Machine: <host> (Apple M5 Max, macOS
27.0.1). Amrit gave no answers in this wave (none were asked). Raw runs: `docs/data/v031/`.
Test clips: the repo's own fixtures `tests/fixtures/tiles_dxt5.mov` (DXV3 alpha) and
`tiles_h264.mp4` (opaque); no media of Amrit's was used.

## READ (sources)

| Fact | Source |
|---|---|
| winit's view takes every `keyDown:` (`interpretKeyEvents:` then its own `KeyboardInput` queue) and never calls `super`; its `sendEvent:` swizzle only reroutes ⌘-keyUp | winit 0.30.13 `src/platform_impl/macos/view.rs:441-496`, `app.rs:21-50` |
| A local monitor "receives copies of events the system posts to this app prior to their dispatch"; only events "dispatched through the application's sendEvent(_:)"; the handler may "return nil to stop the dispatching of the event" | Apple, `NSEvent.addLocalMonitorForEvents(matching:handler:)` |
| `NSMenu performKeyEquivalent:` "returns true if event is a key equivalent that the menu should handle"; nothing documented about disabled items (measured below: a disabled item does not take it) | Apple, `NSMenu.performKeyEquivalent(with:)` |
| `NSAlert runModal` "runs the alert as an app-modal dialog and returns the constant that identifies the button clicked" | Apple, `NSAlert.runModal()` |
| Swift Glance shows its alerts with `NSAlert()` + `runModal()`, style informational / warning, buttons "Set as Default" / "Not Now" or "OK" | `.ref/glance` a9dc601 `Sources/Glance/DefaultHandlerControl.swift:90-104, 125-147` |
| rfd 0.18 without a parent: `utils::sync_pop_dialog` (a CFUserNotification alert, shown by UserNotificationCenter); with a parent: `beginSheetModalForWindow` (a sheet) | rfd 0.18.0 `src/backend/macos/message_dialog.rs:140-166` |
| rfd on Windows without `common-controls-v6` uses `MessageBoxW` | rfd 0.18.0 `src/backend/win_cid/message_dialog.rs:9-23, 204-210` |
| `localizedStandardCompare:` "compares strings as sorted by the Finder … uses the current locale … may be changed in future releases" | Apple, `NSString.localizedStandardCompare(_:)` |
| Swift's folder scan: `contentsOfDirectory(…, options: [.skipsHiddenFiles])`, `UTType(filenameExtension: ext.lowercased())` conforming to `.movie` / `.quickTimeMovie` / `.mpeg4Movie`, sorted by `localizedStandardCompare` of the name | `.ref/glance` a9dc601 `Sources/Glance/PlayerModel.swift:233-268` |
| `UTType.conforms(to:)`: "true if the type directly or indirectly conforms to type, or if it's equal to type"; `UTType(filenameExtension:)`: may give a **dynamic** type for an unknown extension (which conforms to nothing of ours), `nil` for invalid input; "you can't always derive the type of a file system item based solely on its filename extension" (Swift does exactly that; kept) | Apple, `UTType.conforms(to:)`, `UTType.init(filenameExtension:conformingTo:)` |
| `StrCmpLogicalW`: digits by value, not case-sensitive, returns −1 / 0 / 1; "can change from release to release"; Shlwapi.dll 5.5+ | Microsoft Learn, `StrCmpLogicalW` |
| `TaskDialogIndirect` needs Comctl32.dll **version 6** (an app gets it from its manifest) | Microsoft Learn, `TaskDialogIndirect` (Requirements) |
| No source documents `localizedStandardCompare` on digit runs past 64 bits; a 2003 report saw overflowing numbers sort below all others (32-bit then) | web search 2026-11-15: earthlingsoft.net "unnatural order" (2003), Directory Opus forum (a different product, 2^64 limit) |

## DIAGNOSE (RED, v0.3.0 installed at `/Applications/Glance Sample.app`)

1. **Bare keys** (`docs/data/v031/red-keys.txt`): Space, Space, A on `tiles_dxt5.mov`, then A on
   `tiles_h264.mp4`, through System Events → **0 PlayPause, 0 ToggleAlphaType, 0 alpha changes**.
   Cause as the g1-findings brief read it (winit never calls `super keyDown:`, so AppKit never
   offers a bare key to the menu); C3's DIAGNOSE 5 claim ("taken before the view's keyDown") was
   wrong and was never measured.
2. **Alerts** (`red-alert.txt`): Glance Sample ▸ Set … as Default… → the alert is
   `owner=UserNotificationCenter pid=92864 layer=8 260x192`; the app's pid has only its main
   window; Accessibility sees no alert in the app; after `kill -9` of the app the alert **stays**
   (dismissed by hand after each run, nothing left on screen).
3. **Kit STAGE_DIR** (`red-stage.txt`): the default `STAGE_DIR` is
   `/var/folders/…/T//glance-kit/<id>`; LaunchServices records the staged bundle as
   `/private/var/folders/…`; `ls-cleanup.sh` → "left alone (not a build product or DMG copy)",
   two records, exit 1. (Reproduced by registering a copy in the default STAGE_DIR with
   `lsregister -f`, as notarisation / staple / Gatekeeper do; the stale record was removed after.)

No contradiction with the brief.

## WHAT SHIPPED

1. **Bare-key menu shortcuts** — `glance_platform::install_menu_key_monitor()`, installed by
   `glance_ui::run_shell` on macOS. Chosen over mapping egui keys to `MenuCommand` in `run_shell`
   because the monitor (a) keeps the menu the one place that decides (enabled state, checks, the
   same item a click fires), (b) drops the press before winit, so the view never sees it — the
   mapping option would have to filter egui's input after the fact, (c) needs no per-app key table.
   Rules: only presses without ⌘ ⌃ ⌥ ⇧ (⌘ keys already go to the menu first); not while a modal
   window is up (alerts and the open panel keep their keys); key repeats are offered too, as
   AppKit does for Swift Glance. An app with text fields would need them to take the press first;
   the family's windows have none (noted in the doc comment).
2. **Alerts** — `glance_platform::alert(style, title, text, buttons) -> Result<usize>`: macOS an
   `NSAlert` with `runModal` in the app's process (the app's icon, Swift's styles and buttons);
   Windows `MessageBoxW` owned by `GetActiveWindow()`. `glance_ui::dialog::{inform, warn, ask}`
   keep their signatures; the open panel stays rfd. `MOVIE_EXTENSIONS` moved to `glance-platform`
   (re-exported from `glance_ui::dialog`, same path for apps).
   Windows deviation: TaskDialog (Swift's button names) needs comctl32 v6 from a manifest, which
   comes with the Windows installer at C4; until then the two-button alert is OK / Cancel (rfd's
   Windows behaviour in v0.3.0 was the same).
3. **Kit** — `lib.sh` creates `STAGE_DIR` and replaces it with `pwd -P` before anything uses it;
   README's slot table says so. glance-sample's `app.conf` never had a workaround; it now works
   with the default `STAGE_DIR` (proven by this wave's release). glance-rs can delete its line.
4. **Folder listing** — `glance_platform::movie_files(dir) -> Result<Vec<PathBuf>>` (Swift's whole
   listing), with `localized_standard_compare(a, b)` and `conforms_to_movie(ext)`; Windows:
   `read_dir`, hidden attribute skipped, `MOVIE_EXTENSIONS` (any case), `StrCmpLogicalW`; iOS:
   `Unsupported`. The sort is a small merge sort of our own (`sort_by_os`): the OS comparison is
   not promised to be a total order and std's sorts may panic on one that is not (never-crash rule);
   it always returns every item once (unit test with a comparison that is no order).
5. glance-sample logs every key press the window receives (`"event":"key"`), so a run shows which
   presses the menu took. Workspace version 0.3.1.

## MEASURED (GREEN, v0.3.1 notarised, installed from the kit's default STAGE_DIR)

- **Release** (`release.txt`, `KIT_STAGE_DIR` unset): universal, signed, app and DMG Accepted +
  stapled, DMG sha256 `b944c934…5d36` (built from d96e6b7); ls-cleanup: "unregistered build product:
  /private/var/folders/…/Glance Sample.app" → **exactly one record**, exit 0.
- **Install** (`install.txt`): Gatekeeper accepted (Notarized Developer ID) on the DMG's app and
  the installed copy, staple valid, **exactly one record**, exit 0.
- **Keys** (`green-keys.txt`): Space ×2 → **2 PlayPause**, window Space presses **0**; A on DXV
  alpha → **1 ToggleAlphaType, 1 alpha change** (premultiplied → straight), window A presses **0**;
  `l` ×2 control → 2 window presses (keys the menu does not own still reach the window); A on the
  opaque clip → **no menu command, no alpha change** (the disabled item does not take it; the
  window gets the A and does nothing with it, as Swift's view).
- **Alert** (`green-alert.txt`): `owner=Glance Sample pid=16705 layer=8 260x192` — a window of the
  app's pid; Accessibility reads it as "dialog" with its text; after `kill -9` **no alert left**.
- **STAGE_DIR, the RED script again** (`green-stage.txt`): the staged copy is now "unregistered
  build product", one record, exit 0.
- **Folder listing** (`crates/glance-platform/tests/folder.rs`, 4 tests): Finder's order on 22
  names, in macOS 27.0.1 (en): `a, b, B, clip 1, clip 1a, clip 2, clip 02, Clip 3, clip 10,
  clip_1, clip-1, clip.1, eclair, Eclair, éclair, fig, n 18446744073709551616, n 3,
  n 99999999999999999999999, n 18446744073709551615, Ω omega, ж zhe` — case after the letter,
  "2" before "02", space before `_ - .`, accent after the plain letter, Latin < Greek < Cyrillic.
  **Surprise, recorded:** digit runs past 2^64 compare by value modulo 2^64 (2^64 sorts before 3;
  2^64 and 2^65 compare Equal; ties then broken so 2^64+1 > 1); undocumented anywhere found online;
  it is Finder's order, so it is kept. The CI runner's macOS gave the same order on all 22 names.
  Hidden: a dot file and a `chflags hidden` file are left out, `.txt` / `.png` too. Missing folder →
  error value.
- **Type test, macOS 27.0.1**: movies = mov qt mp4 m4v avi mkv webm mpg mpeg m2v mts m2ts ts dv
  3gp flv wmv (and MOV); not movies = mk3d, txt, png, wav, mp3, gif. The CI runner's macOS says
  **mkv is not a movie** (first CI run failed on it): the set moves between macOS versions, so the
  test asserts only the QuickTime / MPEG-4 family in and non-movies out, and prints the rest.

## DEVIATIONS / FOR AMRIT

- None needing a decision. Windows two-button alerts read OK / Cancel until C4's manifest (above).
- Verifier (cc-kit) on the draft: 35 confirmed, 4 unproven (local gates it was not allowed to
  re-run — re-run here, receipts below; tag / note / mailbox copy then still pending), 2 wrong
  (the UTType docs were not cited — added to READ; the header claimed the tag before it existed —
  the tag is now made last). Its notes led to two fixes in d96e6b7: an alert stopped without a
  button was read as the first button (`ask` would have said yes) — now an error; the kit created
  `STAGE_DIR` before refusing one inside / beside the project — now it refuses first. The CLAUDE.md
  row gained the new Windows calls. The sample was then released, installed and measured again
  from d96e6b7 (the numbers above are that run's).
- For glance-rs: Esc is its own rule (G1b); the parity line "Esc quits only when…" in
  `docs/02-PARITY.md` is unchanged here — G1b will send glance-core a one-line request.

## RECEIPTS

```
$ git log --oneline -4   (before the closing commit)
d96e6b7 v0.3.1: verifier corrections (alert without a button is an error, kit refuses a bad STAGE_DIR before creating it, docs)
435920b v0.3.1: the type test asserts what every macOS agrees on, records the rest
a56690a v0.3.1: bare-key menu shortcuts, in-process alerts, kit STAGE_DIR physical, folder listing (RED runs recorded)
ae8930d C3: checkpoint closed

(re-run on d96e6b7)
$ cargo fmt --all --check                                   -> FMT OK (no output)
$ cargo clippy --workspace --all-targets --release -- -D warnings   -> no warnings
$ GLANCE_REQUIRE_GPU=1 cargo test --workspace --release      -> 25 test binaries "test result: ok", passed 71 failed 0
$ cargo deny check                                           -> advisories ok, bans ok, licenses ok, sources ok
$ cargo build --release                                      -> Finished `release` profile [optimized + debuginfo]
$ cargo clippy -p glance-platform --target x86_64-pc-windows-msvc --release -- -D warnings  -> Finished (clean)
$ cargo check --workspace --target aarch64-apple-ios --release                               -> clean

$ gh run view 43762769333   (CI on 435920b)
success  lint: success  windows: success  macos: success (incl. "the x86_64 binary loads under Rosetta 2")
$ gh run view 43762489267   (CI on a56690a) -> macos: failure, "mkv should be a movie on macOS 27" (fixed in 435920b)

$ tools/package/install.sh crates/glance-sample/bundle/app.conf   (tail)
==> installed /Applications/Glance Sample.app (0.3.1)
/Applications/Glance Sample.app: accepted
source=Notarized Developer ID
==> OK: exactly one record (/Applications/Glance Sample.app)
exit 0

green-keys.txt:  == counts: PlayPause=2 ToggleAlphaType=1 alpha-changes=1 window-keys: Space=0 A=1 L=2
                 (A=1 is the press on the opaque clip, where the item is disabled)
red-keys.txt:    == counts: PlayPause=0 ToggleAlphaType=0 alpha-changes=0
green-alert.txt: owner=Glance Sample pid=16705 layer=8 size=260x192 ; after kill -9: (no window)
red-alert.txt:   owner=UserNotificationCenter pid=92864 layer=8 size=260x192 ; after kill -9: still there
```

Raw files (`ls -la docs/data/v031/`) are listed in the mailbox copy's receipts at close.

## NEXT

- glance-rs: bump the pin to `v0.3.1`, delete `crates/glance/src/folder.rs`'s stand-in
  (`natural_cmp` + the extension list) for `glance_platform::movie_files`, delete the
  `STAGE_DIR` workaround in `crates/glance/bundle/app.conf` (back to glance-sample's plain default; note: `~/claude-handoffs/glance-rs_2026-11-15_note-from-glance-core-v031.md`).
- The sample stays installed at `/Applications/Glance Sample.app` (v0.3.1); remove with
  `rm -r "/Applications/Glance Sample.app"` and `lsregister -u` of that path when no longer wanted.
