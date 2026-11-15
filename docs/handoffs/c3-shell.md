# glance-core C3 — the shell (handoff)

Brief: `~/claude-handoffs/glance-core_2026-11-15_c3-shell_brief.md`. Machine: <host> (Apple M5
Max, macOS 27.0.1). Measured results: `docs/06-C3-RESULTS.md` (raw in `docs/data/c3-*`). Amrit
gave no answers in this wave (none were asked).

## DIAGNOSE

Sources are Swift Glance a9dc601 (`.ref/glance`, paths relative to it) unless named.

### 1. Settings keys (UserDefaults)

Swift Glance (`UserDefaults.standard` = domain `com.amritus.glance`; not sandboxed,
`scripts/entitlements.plist:5-8`):

| Key | Type / encoding | Default | Source |
|---|---|---|---|
| `Glance.backgroundMode` | String `"black"` / `"checkerboard"` (raw values of `DXVRenderer.BackgroundMode`) | black, also for missing or unknown | `Sources/Glance/PlayerModel.swift:63-71`; `Sources/GlancePlayback/DXVRenderer.swift:75-78` |
| `Glance.alphaTypeByClip` | [String], each `"<64-hex clipKey>=<premultiplied\|straight>"`, newest last, cap 2,000, oldest dropped; read = last line with the key's prefix; a stored value that is not all strings reads as empty | absent | `Sources/Glance/AlphaTypeMemory.swift:10-26` |
| `Glance.defaultHandlerPromptShown` | Bool (`set(true)`, read with `bool(forKey:)`) | false | `Sources/Glance/DefaultHandlerControl.swift:25,43-50` |

Not persisted: volume, mute, loop, last file (`PlayerModel.swift:76-78`; no write found). For the
store design only: CueGlance stores `CueGlance.backgroundMode` (String), `CueGlance.alphaTypeByClip`
as an **array of 2-string arrays** `[clipKey, type]` (not Glance's `key=value` lines),
`CueGlance.defaultExportDirectory` (String path), `CueGlance.recentProjects` ([String], max 5)
(`docs/research/cueglance.md` §6; `cueglance/Sources/CueGlance/AlphaTypeMemory.swift:10-32`,
`CueDocument.swift:376,457`); GlEnc 20 `glenc.*` keys of String, Int, Bool and **Data (JSON)**
(`docs/research/glenc.md` §7.2; `glenc/Sources/GlEnc/AppSettings.swift:291-397`); MaCrate
`Crate.*` keys of [String], String, Double, Bool, Float, Data(JSON) through `CrateDefaults.app`,
which uses `.standard` inside the bundle and the named suite outside it because
`UserDefaults(suiteName:)` refuses the main bundle's id (`crate/Sources/CrateKit/CrateDefaults.swift:16-34`).
So the store holds every plist type, nested arrays and data included, and opens a domain as
standard-or-suite the same way (`glance_platform::Defaults::for_domain`).

### 2. The look as numbers

`TransportBar.swift`: height 40 (`:14`), min width 500 (`:21`), `HStack(spacing: 2)` (`:24`),
padding 10 (`:100`), background `Color(white: 0.08)` (`:103`), top hairline 1 pt white 6 % (`:104-106`),
separators 1 × 18 white 8 % padded 4 (`:109-114`); `IconButton`: SF Symbol 13 pt `.medium`, colour
white 0.78, lit colour `.blue`, frame 28 × 28, hover fill white 12 % rounded 5 (`:155-170`); dimmed
prev / next opacity 0.3 (`:34-35`); volume slider `.mini`, width 90, tint white 0.75 (`:71-77`);
Alpha Type control 11 pt, hint 10 pt grey 0.5, stated 11 pt grey 0.45, spacing 5, leading 6
(`:79-80,185-215`); `Spacer(minLength: 8)`; readout 10 pt monospaced grey 0.5, tail truncation
(`:89-94`). Symbols: `backward.end.fill`, `play.fill` / `pause.fill`, `forward.end.fill`,
`arrow.counterclockwise`, `gobackward.10`, `goforward.10`, `backward.frame.fill`,
`forward.frame.fill`, `repeat`, `speaker.slash.fill` / `speaker.wave.{1,2,3}.fill` (thresholds
0.0001 / 0.33 / 0.66, `:118-125`), `arrow.up.left.and.arrow.down.right`. `ScrubBar.swift:12-31`:
`HStack(spacing: 12)`, padding 12, labels 11 pt monospaced white 85 %, widths 64 (trailing) and 56
(leading), time `%d:%06.3f`; row 36 pt, background grey 0.08 (`PlayerScreen.swift:36-38`). Message
overlay: `.headline` + `.caption` secondary, spacing 6, padding 20 (`PlayerScreen.swift:20-35`).
Window: default `960 × (576 + 40)`, min `500 × 180`, `.windowStyle(.titleBar)` (`GlanceApp.swift:44,64-66`);
fit rule `PlayerScreen.swift:106-144` (video area = content − title-bar overlap − (36 + 40); width =
area × aspect, min `max(contentMinSize, 500)` then max 95 % of `visibleFrame`); ⌘0
`PlayerScreen.swift:153-173`. Fonts: SwiftUI's system font (no custom font in the target).
egui can't draw SF Symbols (they're Apple's, only through AppKit / SwiftUI), so it draws the same
pictures as vector shapes. Everything in `crates/glance-ui/src/look.rs`.

### 3. Menus, shortcuts, open paths, default handler, media keys

Menus (`GlanceApp.swift:67-164`): File `Open…` ⌘O (one file, no folders, movie types); app menu
`Set Glance as Default for .mov Files…` after About; View `Default Window Size` ⌘0, `Background` ▸
Black / Checkerboard, `Alpha Type` ▸ Premultiplied / Straight (enabled for DXV alpha only),
`Toggle Alpha Type` **A** (bare); AppKit's Enter Full Screen ⌃⌘F; Playback `Play/Pause` **Space**
(bare), Previous / Next Clip ⌘← / ⌘→, Restart, Volume Up / Down ⌘↑ / ⌘↓, Mute, Loop (no shortcut:
⌘M is Minimize, `:128-129`). Keys in the view: `VideoHostingView.swift:204-270` (arrows ±5 s, Shift
±0.5 s, ↑ ↓ volume, `,` `.` step, `m`, `-` `=`, `l`, `f`, `[` `]`, Esc rule). Open paths: ⌘O; drop
on the view, first URL, any type (`VideoHostingView.swift:274-285`); Finder / Open With / Dock /
`open` → `application(_:open:)`, last URL, posted to the one window (`GlanceApp.swift:195-204`), cold
launch `.onOpenURL` (`:46-54`); recents via `noteNewRecentDocumentURL`. Default handler
(`DefaultHandlerControl.swift`): `urlForApplication(toOpen: com.apple.quicktime-movie)` == bundle
URL; `setDefaultApplication(at:toOpen:)`; first-launch alert 0.5 s after launch "Make Glance the
default for .mov files?" / Set as Default / Not Now, asked once; menu alerts "already the default",
"is now the default", "Couldn't set Glance as default." + manual instructions. Media keys
(`NowPlayingController.swift:30-101`): toggle / play / pause / previous / next track (= clip) /
change playback position enabled; skip and seek commands disabled; Now Playing title, duration,
elapsed, rate, playback state.

### 4. The release pipeline

`scripts/build.sh` refuses when `CFBundleVersion` ≠ `CFBundleShortVersionString` (`:23-30`), then
assembles the app; `sign.sh`: `codesign --force --deep --options runtime --timestamp
--entitlements`, verify, spctl (`:24-38`); `notarize.sh`: `ditto -c -k --keepParent` →
`notarytool submit --keychain-profile "NotaryBuilds" --wait` → `stapler staple` / `validate` →
spctl (`:7-41`); `make-dmg.sh`: stage app + `/Applications` link, `hdiutil create -format UDZO`,
volume `Glance <v>`, file `Glance-v<v>.dmg`, codesign the DMG, `ls-cleanup.sh` (`:20-54`); the DMG
itself is notarised and stapled and `.sha256` written after stapling by hand
(`docs/handoffs/glance_2026-10-09_release-v0171-results.md:15`); `ls-cleanup.sh`: targeted
`lsregister -u` of the build product and `/Volumes/Glance*` copies, exit 1 unless one record in
`/Applications` (`:20-60`); `install-from-dmg.sh` (`:27-62`). Keychain: Glance's profile name
`NotaryBuilds` is **ABSENT** on this Mac; `amritus-notary` (crate, cueglance, glenc scripts) is
PRESENT and was used; one Developer ID Application identity PRESENT. No secret printed: the kit
finds the identity by hash at run time and masks identities and team ids in its output.

### 5. muda, rfd and the open-file hook with winit 0.30

- **winit's delegate.** winit 0.30.13's macOS module docs say "Winit guarantees that it will not
  register an application delegate" (`src/platform/macos.rs:29`), but its source registers
  `WinitApplicationDelegate` in `EventLoop::new` (`src/platform_impl/macos/event_loop.rs:232-240`,
  `app_state.rs:47-74`) and relies on its `applicationDidFinishLaunching:`; its changelog removed
  custom delegates on **iOS** only (`changelog/v0.30.md:40`). Code wins: replacing the delegate (the
  documented pattern) would stop winit launching. The accepted way is to add
  `application:openURLs:` to winit's class with `class_addMethod` after `EventLoop::new` and before
  `run_app`; AppKit checks `respondsToSelector:` when the open event arrives (at cold launch during
  `finishLaunching`, before `resumed`). `class_addMethod` never replaces, so a winit that adds the
  method later is detected (`install_open_handler` errors).
- **muda.** Build the menu once the event loop exists, `init_for_nsapp()` then (muda docs and
  README: create in `StartCause::Init` / `resumed`, events through `MenuEvent::receiver`); Windows
  `init_for_hwnd` is `unsafe` (muda 0.21.2 `items/menu.rs:347`), so it sits in glance-platform.
  Bare-key shortcuts (Space, A) map to key equivalents with no modifier
  (`accelerator/macos.rs`), as SwiftUI's; menu key equivalents are taken before the view's keyDown,
  so Space fires once.
- **rfd.** Sync dialogs on the main thread inside the event-loop callback (`NSOpenPanel` /
  `NSAlert` run modal); no default features (the defaults are Linux portal / GTK).
- Searched online (winit docs on docs.rs, muda's docs / README): no known conflict beyond the
  delegate contradiction above.

No contradiction with the brief.

## What shipped

- **glance-platform** (new; the one crate with `unsafe`, own lints incl.
  `undocumented_unsafe_blocks`): macOS preferences domain, open-file hook, default handler, media
  keys + Now Playing, recents, window subtitle; Windows single instance (named mutex + pipe), menu
  bar, SMTC media controls; the settings folder; one event queue with a waker.
- **glance-settings** (new): `Store` (Defaults / `FileStore` JSON on Windows / memory), Swift's
  read rules, `GlanceSettings` (three keys, cap 2,000).
- **glance-ui**: system font (egui's own fonts gone), `look`, `icons`, `bar` (transport bar, scrub
  row, Alpha Type control, message overlay, text formatters), `menu` (muda), `dialog` (rfd, Swift's
  words), `window` (fit rule), `run_shell` (menu bar, min size, platform / menu / drop events,
  title, subtitle, fit, default size, full screen).
- **tools/package/** (new): the kit, with an adoption checklist (`tools/package/README.md`).
- **glance-sample** (new, throw-away `com.amritus.glancecore.sample`): built, signed, notarised,
  stapled, DMG, installed in `/Applications`.
- Carry-overs: audio reroute (`AudioClock::follow_device`, cpal error classes), `alpha_rule` from
  `MediaInfo::alpha`, system font + deny exception removed.
- `glance-proof reroute`; `tools/c3-open-check.sh`, `tools/c3-look-stats.py`,
  `tools/swift-settings/harness.swift`. Workspace 0.3.0.

## Validated (evidence: `docs/06-C3-RESULTS.md`)

- Settings round-trip with Swift Glance's own code, both ways (§1).
- Finder opens at cold launch and while running into one window, one process; last of two wins (§2).
- Fit 540 / 500 / 960 for 1:1 / 9:16 / 16:9, height kept (§3).
- Bar and scrub row equal to Swift's in heights and greys; captures for G2 in `docs/data/c3-look/` (§4).
- Universal, hardened, app and DMG notarised (Accepted) and stapled, `.sha256`, one LaunchServices
  record; version mismatch refused (§5).
- Reroute: speed 0.9988 / 0.9996 after a 44.1 → 48 / 96 kHz change (resolution ±0.2 %), one rebuild of 12–43 ms (§7).
- Native alpha stated (§8; the test asserts `Stated(_)` and prints Straight; HEVC with alpha not
  covered by a fixture). Gates: 66 tests passed, 0 failed, GPU required (§9). Parity: 5 new ticks,
  the rest of the named sections annotated with core halves (`docs/02-PARITY.md`).
- Verifier (cc-kit): 26 confirmed, 2 unproven, 6 wrong at ed5ab5b; the wrong ones corrected in the
  next commit (a tick that was G1's, the 96 kHz blocked-call figure, the icon count, stale
  checkpoint lines, the tag status, this observation's evidence). Unproven and stated: the sample's
  playback is evidenced by its log (open, frame 0 shown) and the captures, not by a measured run;
  the online search for muda / winit conflicts is described, not recorded.
- Tag `v0.3.0`: made on the commit after the verifier's corrections, once CI is green on it (the
  mailbox copy's RECEIPTS show it).

## Deviations and findings

- The C2 rate-change concern did not reproduce: cpal's DefaultOutput unit already kept the speed by
  converting a second time; C3 rebuilds at the device rate (one conversion). C2 also treated every
  cpal error, under-runs included, as fatal — fixed.
- Windows settings location: a JSON file in `%APPDATA%` rather than the roadmap's "registry" (one
  documented place as the brief asks; the family's values include nested arrays and data, which a
  registry value does not hold); roadmap updated.
- Window chrome differs from Swift (title bar 32 pt above the content vs SwiftUI's 28 pt over it):
  outer 960 × 648 vs 911 × 616 on a 16:9 clip. Left for the G2 look review with the numbers.
- The fit uses winit's monitor width; Swift uses `visibleFrame` (they differ with the Dock at a side).
- Swift Glance's reference build (throw-away id, ad-hoc signed, from the scratchpad) opened no window
  when cold-launched with a file through `open -a` (its windows were listed 0 × 0, off screen);
  launched directly it did. Observed in the session, no data file kept; not investigated (not this
  repo's; may be the reference build's setup).
- Notary profile `amritus-notary` used (Glance's `NotaryBuilds` absent).
- DMG 11.6 MB (Glance 0.18.1: 1.06 MB): the release profile keeps `debug = 1`; stripping is a
  decision for each app's release.

## Not proven / open (for G1 and later)

- Media keys reaching the app (registered and publishing; no key pressed by hand).
- Recents read back from the OS; drag-and-drop onto the window (code only); first-launch prompt
  (words, flag; not shown in the sample).
- The `.mov` "Set as Default" switch (proven only on the sample's own type).
- Windows code: compile-checked (glance-platform locally, the workspace by CI), not run (C4).
- The sample stays installed at `/Applications/Glance Sample.app` for Amrit to try; remove with
  `rm -r "/Applications/Glance Sample.app"` then
  `/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -u "/Applications/Glance Sample.app"`,
  and `defaults delete com.amritus.glancecore.sample`.

## Commits

See RECEIPTS in the mailbox copy (`~/claude-handoffs/glance-core_2026-11-15_c3-shell-result.md`).
