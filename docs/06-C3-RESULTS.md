# C3 results — the shell (macOS)

2026-11-15, core wave C3, on the core's home machine (<host>: Apple M5 Max, macOS 27.0.1, Dark
appearance, default output MacBook Pro Speakers at 44.1 kHz). Release builds only. Raw data:
`docs/data/c3-sample/`, `docs/data/c3-reroute/`, `docs/data/c3-look/`. Test media: the repo's
fixtures and, read-only, Amrit's folder `<testfiles>`
(`_master_hapm_512x512.mov`, `_master_hapm_540x960.mov`, `_master_hapm_3840x2160.mov`,
`DXT5-DXV3.mov`). Nothing was done under Swift Glance's bundle id: the sample is
`com.amritus.glancecore.sample`, the Swift reference build for the look captures
`com.amritus.glancecore.swiftref` (unregistered and its domain deleted after: its plist reads `{}`).

## 1. Settings in the Swift apps' store

`glance-settings` over `glance-platform::Defaults` (NSUserDefaults by domain).

- **Swift writes, Rust reads; Rust writes, Swift reads** — `swift_glance_and_the_rust_store_read_each_others_settings`
  runs Swift Glance's own settings code (`tools/swift-settings/harness.swift`: `AlphaTypeMemory`,
  the `backgroundMode` raw values, the prompt flag, copied from a9dc601) against the throw-away
  domain. Swift writes checkerboard, the prompt flag and 2,004 Alpha Type choices (its cap keeps
  2,000); Rust reads the background, the flag, the cap and the newest-line-wins choices. Rust
  writes black and four choices; Swift reads `{"alphaLines":2000,"background":"black","choices":{"0":"none","1":"none","4":"premultiplied","7":"straight","9000":"premultiplied","9001":"premultiplied",…},"promptShown":true}`.
  The other apps' value shapes come through typed both ways: nested string arrays (CueGlance's
  alpha memory), `Data` (GlEnc's JSON keys), `Int`, `Double`, `Float`.
- Swift's read rules are reproduced: `bool(forKey:)` on numbers and strings (`"YES"`, `"007"`,
  `"-0"`…), `stringArray(forKey:)` refusing a mixed array, an unknown background raw value as black
  (`booleans_read_as_swift_reads_them`, `the_background_is_black_unless_checkerboard_is_stored`,
  `alpha_choices_follow_swifts_memory_newest_last_capped`).
- Every plist type round-trips through a real domain (`every_plist_type_round_trips_through_a_domain`).
- The installed sample reads and writes its own domain: `defaults write … checkerboard` → the app
  read `"background":"checkerboard"` at launch; the app's `--toggle-background` → `defaults read`
  gives `black` (`docs/data/c3-sample/fit-settings.txt`).
- Windows: `FileStore`, one JSON file per app at `%APPDATA%\Amritus\<App>\preferences.json`, every
  type kept across a reopen (`the_file_store_keeps_every_type_across_a_reopen`, run on macOS);
  not run on Windows (C4).

## 2. Files from Finder into one window

`glance_platform::install_open_handler` adds `application:openURLs:` to winit's own application
delegate (winit 0.30.13 registers `WinitApplicationDelegate` in `EventLoop::new`, contrary to its
module docs; replacing it would lose `applicationDidFinishLaunching:`). On the installed, notarised
sample (`tools/c3-open-check.sh`, `docs/data/c3-sample/open-check.txt`):

| Step | Process | Windows on screen | What the app logged |
|---|---|---|---|
| cold launch, `open -a <app> tiles_dxt1.mov` (an Apple Event; argv empty: `"args_file":null`) | one pid | 1 (960×648) | `os-open` → `open` (DXV3 Normal Quality) → `shown` frame 0 |
| while running, `open -a <app> tiles_hapy.mov` | the same pid | 1 | `os-open` → `open` (HAP Q) → `shown` |
| two files at once, `tiles_h264.mp4 tiles_dxt1.mov` | the same pid | 1 | `os-open` with both paths → opens the last (Swift's rule) |

Window counts are `CGWindowListCopyWindowInfo` (layer 0, on screen) for the pid.

## 3. The window rules

On each clip open the width follows the clip at the current height, clamped to the bar's 500 and
95 % of the screen (`window::fit_width`, unit test with Swift's cases; measured on the installed
sample, outer sizes, the title bar 32 pt):

| Clip | Expected content | Measured outer |
|---|---|---|
| `_master_hapm_512x512.mov` (1:1) | 540 × 616 | 540 × 648 |
| `_master_hapm_540x960.mov` (9:16) | 500 × 616 (minimum) | 500 × 648 |
| `DXT5-DXV3.mov` (16:9) | 960 × 616 | 960 × 648 |

The height never changed. Minimum content size 500 × 180 is a window attribute; View ▸ Default
Window Size (⌘0) restores 960 × 616 centred (`Host::default_size`).

**Different from Swift, for the look review (G2):** Swift Glance's window has SwiftUI's title bar
laid *over* the top 28 pt of its 616-pt content (its fit rule subtracts that overlap: video area
616 − 28 − 76 = 512, width 911 for 16:9); the Rust window has the standard title bar (32 pt)
*above* a 616-pt content (video area 540, width 960). Outer: Swift 911 × 616, Rust 960 × 648 on
the same clip. The fit rule is the same formula with the overlap 0.

## 4. The look against Swift Glance

Side-by-side captures on `DXT5-DXV3.mov` at the default size: `docs/data/c3-look/` (for Amrit's
review in G2; Claude judged numbers, not the pictures). Bands from the bottom of each window, the
most common colour across each row (`tools/c3-look-stats.py`, `docs/data/c3-look/stats.txt`):

| Band | Swift Glance | Rust (glance-sample) |
|---|---|---|
| transport bar | 39 pt, grey 20 | 39 pt, grey 20 |
| its top hairline | 1 pt, grey 34 | 1 pt, grey 34 |
| scrub row | 16 + 4 (track, grey 44) + 16 pt, grey 20 | 16 + 4 (track, grey 44) + 16 pt, grey 20 |
| video area | 512 pt, black | 539.5 pt, black |
| title bar | 28 pt, grey 46, over the content | 32 pt, grey 35 (+ 0.5-pt line), above it |

The scrubber's unfilled track was white 20 % (grey 67) in the first capture; set to white 10 %
from this measurement. Fonts: the system font read from the OS (SF Pro, weights by `wght`: 400 /
510 / 590; SF Mono), egui's own fonts no longer linked. Icons: 13 icons standing for the bar's 15 SF Symbols (the speaker's three wave levels are one
icon with a level), drawn as vector shapes (`glance_ui::icons`; SF Symbols are drawable only through AppKit / SwiftUI) — a look-review
item. Menus are the OS's (muda); egui draws tooltips and the Alpha Type popup — look-review items.

## 5. The packaging kit on the sample

`tools/package/release.sh` then `install.sh` (`docs/data/c3-sample/release.txt`, `install.txt`;
identities masked):

- Universal: `Mach-O universal (x86_64 arm64)`, `minos 14.0`; the Intel slice runs under Rosetta 2
  (`arch -x86_64 … --toggle-background`, exit 0; `rosetta.txt`).
- Signed with the hardened runtime (`flags=0x10000(runtime)`), secure timestamp, Swift Glance's two
  entitlements (`signature.txt`).
- App notarised: submission `24f683ab-9d10-4b59-84b9-4c44e5b2169b` **Accepted**, stapled, `The
  validate action worked!`, `spctl`: `accepted`, `source=Notarized Developer ID`.
- DMG `Glance Sample-v0.3.0.dmg`, 11,632,812 bytes: signed, submission
  `25444721-800f-42c3-9b54-6adb0e40e074` **Accepted**, stapled, `accepted` / `Notarized Developer
  ID`; sha256 `2b4a5c889cdf0d303eb8b97fdbaec235b6e06977843c2288370d7e3a0712adff` in the `.sha256`.
- Installed from the DMG into `/Applications`: Gatekeeper accepted, staple valid; **exactly one
  LaunchServices record** (`/Applications/Glance Sample.app`) after install and ls-cleanup, still
  one after the launches of §2–§3 (`fit-settings.txt`).
- A plist whose two version fields differ is refused (`version-mismatch.txt`, exit 1).
- Notary profile: Swift Glance's scripts name `NotaryBuilds`, which is not in this Mac's keychain;
  the existing profile `amritus-notary` (crate, cueglance, glenc) was used (a slot in `app.conf`).

## 6. Default handler, media keys, recents

- "Set as Default" (macOS): `set_default_for` on the sample's own throw-away type
  `com.amritus.glancecore.sample-clip` (never `.mov`): the OS answered `Ok(())`, `is_default` true
  (`set-default.txt`). The sample already owned that type (rank Owner), so this proves the call and
  its answer, not a switch from another app; the `.mov` switch belongs to Glance's rehearsal (G2).
- Media keys: `MPRemoteCommandCenter` targets registered at launch (`"registered":true` in the
  log) and Now Playing published on open / play / pause / seek. **Not pressed by hand in this
  wave**: that a key press reaches the app is not proven here.
- Recents: `note_recent` is called on every open; the OS list was not read back.

## 7. Audio: the output device changes rate mid-clip

`glance-proof reroute` (`docs/data/c3-reroute/`): the clip's sound plays muted; at 4 s another
thread sets the default output's hardware rate (cpal sets a device's physical format when it builds
a stream at another rate), and the device is put back after. Speed = frames the callback played per
wall second ÷ the rate the samples were resampled for (resolution about ±0.2 %: 50-ms samples over
512-frame callbacks).

| Run | Reroutes | Speed before | Speed after |
|---|---|---|---|
| C2 behaviour (`--no-follow`), 44.1 → 48 kHz | 0 | 0.9995 | 1.0008 |
| C3, 44.1 → 48 kHz | 1 (rebuild 42.8 ms) | 1.0002 | 0.9988 |
| C3, 44.1 → 96 kHz | 1 (11.7 ms) | 1.0012 | 0.9996 |

The C2 concern did not reproduce on this path: cpal builds default-device streams on the
DefaultOutput unit, which converts from the stream's rate to the device's itself — the speed stayed
right, at the cost of a second sample-rate conversion. With C3 the stream is rebuilt at the device's
rate (one conversion, rubato), the position heard kept. The poll costs p50 0.0005 ms, max 0.53 ms
every 250 ms; at the moment of the switch the call blocked 134 ms (48 kHz run) and 223 ms
(96 kHz run) while CoreAudio reconfigured (`follow_max_ms` in the JSON). Found while reading: C2 treated every cpal error as fatal, buffer under-runs
included — now counted, never a failure.

## 8. Native alpha

`alpha_rule` reads `MediaInfo::alpha` (DXVKit Plus 0.9.0): the ProRes 4444 fixture is
`Stated(Straight)`, H.264 and ProRes 422 opaque, DXV with alpha choosable
(`a_native_clip_with_alpha_is_read_as_its_format_states`). The stated mode itself is DXVKit Plus's
Apple backend reading the format description (Swift's `probeAVFoundation` rule).

## 9. Gates

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets --release -- -D warnings`,
`GLANCE_REQUIRE_GPU=1 cargo test --workspace --release` (**66 passed, 0 failed**, none skipped),
`cargo deny check` (the egui font exception removed), `cargo build --release`. iOS check of the
workspace clean locally; Windows: glance-platform checked locally, the workspace by CI (DXVKit
Plus's `zstd-sys` needs MSVC).
