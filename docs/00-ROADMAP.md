# Roadmap — the Glance family in Rust

2026-11-14, W0; updated the same day in core wave C1 with Amrit's answers to the 16 decisions
(`docs/01-RESEARCH.md` §9). Amrit's decisions this plan is built on (verbatim in the W0 brief): egui
everywhere, each app keeping its current look; one shared core repo plus one repo per app; the
Swift apps stay live until their Rust twin reaches parity, then the Rust app takes over the same
name and bundle id; order core → Glance → CueGlance → GlEnc → MaCrate; GlancEhr starts after the
core is built; iCrate stays Swift; no FFmpeg.

The 16 decisions W0 left open are closed in `docs/01-RESEARCH.md` §9, each with Amrit's words
or Cowork's technical ruling; this plan carries them: glance-core is MIT (private until 2026-11-17, then public: Amrit 2026-11-17 08:46, "public");
Windows 11 only; universal macOS builds (arm64 + x86_64); no ProRes on Windows until Apple
answers; the quirks of the Swift apps are fixed, not reproduced; GlancEhr also targets iPhone and
iPad.

## 1. Repos

All under `<workspace>/`, each its own Claude Code window, each private
on GitHub until Amrit says otherwise. glance-core's licence is MIT (Amrit 2026-11-14: "MIT"), and glance-core is
public since 2026-11-17 (Amrit 2026-11-17 08:46: "public"; the history was cleaned first:
`docs/handoffs/clean-history-and-publish.md`). The Windows-only parity items of every app stay open for the Windows
milestone (§7).

| Folder | GitHub | What | Replaces |
|---|---|---|---|
| `glance-core` | `AmritusG/glance-core` | the shared crates (this repo) | — |
| `glance-rs` | `AmritusG/glance-rs` | Glance | `AmritusG/glance` app target; releases stay in `glance-releases` |
| `cueglance-rs` | `AmritusG/cueglance-rs` | CueGlance | `cueglance`; releases in `cueglance-releases` |
| `glenc-rs` | `AmritusG/glenc-rs` | GlEnc + `glenc-cli` | `glenc`; releases in `glenc-releases`, mirror `glenc-public` |
| `macrate-rs` | `AmritusG/macrate-rs` | MaCrate | `crate` (MaCrate target); releases in `CrateRelease` |
| `GlancEhr` | `AmritusG/GlancEhr` | the club player (not started) | — |

Outside the programme but depended on: `AmritusG/DXVKit-Plus` (public, MIT) owns every decoder.
The Swift repos stay as they are; iCrate keeps building from `crate`.

## 2. Crates in glance-core

Built = exists after W0. A crate is created in the wave that fills it.

| Crate | State | Owns | Depends on |
|---|---|---|---|
| `glance-media` | **built (W0, C1)** | content routing with the "<codec> in <CONTAINER> is not supported" message, open, platform backend choice, codec names, the Alpha Type rule, `clip_key`, the alpha-look probe; later: `FrameTiming` | `dxvkit-plus-core`, `dxvkit-plus-apple` (macOS), DXVKit Plus's Windows backend when it exists |
| `glance-gpu` | **built (W0, C1)** | device, a clip's GPU state on DXVKit Plus's exact block path (selectable per view), the Alpha Type pass in whole code values, the 0-difference render harness (`verify`); later: thumbnails (CPU and GPU) | `dxvkit-plus-wgpu`, `wgpu` |
| `glance-playback` | **built (W0, C2)** | the frame clock (audio master with the 0.25 s fallback, the wall clock on DXVKit Plus's transport), loop / scrub / step / seek / restart and end-of-clip rules as Glance has them, a clip's decoded frame behind it, timing statistics | `glance-media`, `glance-audio` |
| `glance-ui` | **built (W0, C1, C3)** | the egui host (winit + egui-winit + egui-wgpu), the video widget, the checkerboard, a headless stage for tests (`headless`: the same widget drawn offscreen, and the software model of the scaled picture); since C3 the shell: the system font, the shared Glance look (colours, spacing, icons), transport bar, scrub bar, menus (`muda`), dialogs (the open panel through `rfd`; alerts through `glance-platform`), window-fit rules, `run_shell` (menu bar, open-file and media events, drops, bare-key menu shortcuts); later: a second-screen output window (for GlancEhr) | `glance-gpu`, `glance-platform`, `egui`, `muda`, `rfd` |
| `glance-audio` | **built (C2)** | device output (cpal) at the device's rate, the first audio track decoded by DXVKit Plus (PCM; AAC through the OS), resampling (rubato), channel mapping, volume / mute, the position heard that the video clock follows | `cpal`, `rubato`, `glance-media` |
| `glance-platform` | **built (C3)** | the one island with `unsafe`: macOS preferences domain (`NSUserDefaults`), open-file events (`application:openURLs:` on winit's delegate), default-handler registration, media keys + Now Playing, recents, the window subtitle; since v0.3.1 bare-key menu shortcuts (an `NSEvent` local monitor), app-modal alerts (`NSAlert`), the folder listing in Finder's order (`movie_files`); Windows (compile-checked, run at C4): single instance + file hand-off (named mutex + pipe), the settings folder (`%APPDATA%`), media transport controls, the menu bar, message boxes, the folder listing (`StrCmpLogicalW`); later: drag-out | `objc2*`, `windows`, `muda` (Windows) |
| `glance-settings` | **built (C3)** | typed settings over the app's store (macOS: its `UserDefaults` domain; Windows: `%APPDATA%\Amritus\<App>\preferences.json`), Swift's read rules, Glance's three keys; later each app's own | `glance-platform`, `glance-media` |
| `glance-sample` | **built (C3)**, not a product | the shell proven in a bundled, notarised app under the throw-away id `com.amritus.glancecore.sample` | all |
| `glance-encode` | **built (v0.5.0, GlEnc goal run)** | GlEnc's encode engine: source routing and readers, trim / crop / resize / letterbox, frame-rate and timescale rules, alpha rules on read and write, audio, the sinks. The queue, naming, collisions and settings stay in glenc-rs (Swift keeps them in the app target too) | `dxvkit-plus-enc`, `dxvkit-plus-apple` (v0.12.0) |
| `glance-library` | planned (MaCrate wave) | MaCrate's SQLite twin (rusqlite, the 12 migrations), sidecars, stash files, `clip_id`, FTS, the folder watcher | `rusqlite` |
| `glance-arena` | planned (MaCrate wave; GlancEhr needs it too) | Resolume Arena REST + websocket client, modes, the Send To menu model | `ureq`, `tungstenite`; written from SendEhrKit and released under MIT (Amrit 2026-11-14: "reuse" — he is the sole author; the relicence is recorded in the crate's header and NOTICE) |
| `glance-proof` | built (W0, C1) | the W0 proof window; `verify` (the render harness over any files, no window) and `hapq` (HAP Q against Swift Glance's convention) | all |

What the core never holds: a decoder, a demuxer, an encoder, a QuickTime writer, OS-codec FFI.
Those are DXVKit Plus's (`docs/01-RESEARCH.md` §6, "who owns what").

Layering (dependencies point down only): `glance-proof` / apps → `glance-ui` → `glance-gpu`,
`glance-playback`, `glance-audio`, `glance-settings` → `glance-media`, `glance-platform` → DXVKit
Plus.

## 3. What the core must cover for GlancEhr

GlancEhr's scope, in Amrit's words of 2026-11-14 (its own scope brief is not on this Mac; Cowork
note `glance-core_2026-11-14_note-from-Cowork-w0-review.md`): drag a folder in; the clips play in
random order with automated transitions (drawn from Arena, his SeamWork plugin, After Effects,
Notch, MadMapper and anything suited to automation); everything adjustable in a settings menu with
Glance's look; Send To fullscreen on a second screen; Windows from day one; **and iPhone / iPad as
a third target** — "plug in their device and run allowed files directly from their device".

| GlancEhr need | Where it lands |
|---|---|
| A folder dragged in | folder scan in Finder's order: `glance_platform::movie_files` (v0.3.1, Glance's folder navigation) |
| Random playback | DXVKit Plus's transport already has random-frame and random-direction modes; `glance-playback` exposes them, plus a playlist layer for "next clip at random" |
| Automated transitions | a two-clip mix pass in `glance-gpu` (two `ClipView`s, one blend uniform; such layers may sit on `BlockPath::Sampled`), driven by `glance-playback` |
| A settings menu with Glance's look | the shared look and `glance-settings` (C3) |
| Second-screen Send To | an app-owned output window on its **own** wgpu surface and pacing in `glance-ui` (never an egui viewport; loombda-rs law 4), borderless fullscreen on a chosen monitor; "Send To" Arena is `glance-arena` |
| Windows from day one | every core crate builds and passes its tests on Windows from the Windows milestone on (section 7); nothing Mac-only outside `glance-platform` and DXVKit Plus's Apple island |
| iPhone / iPad | **the core must not design iOS out.** From C1 every core crate compiles for `aarch64-apple-ios` in CI (`cargo check`, like the Windows check); nothing macOS-only outside `glance-platform` and DXVKit Plus's Apple island (which is to build for iOS too — its brief); wgpu runs on Metal there; touch input and the iOS app shell are GlancEhr's. No iOS app work until GlancEhr's own W0 |

When GlancEhr's own W0 runs it reads this file and `docs/01-RESEARCH.md`; anything it needs that
is missing comes back as a brief to this repo.

## 4. Versions and pins

- **glance-core** is versioned as one workspace: `0.MINOR.PATCH`, tag `vX.Y.Z` on `main`. A minor
  bump may break the API; a patch never does. Until 1.0 every app wave names the core tag it needs.
- **Apps pin the core by git tag**, never a branch, never a path:
  `glance-ui = { git = "https://github.com/AmritusG/glance-core", tag = "v0.3.1" }` (all core
  crates at the same tag). `Cargo.lock` is committed in every app repo. A core change an app needs
  is made in this repo, tagged, then the app bumps its pin in its own commit — the same discipline
  the Swift apps use for Glance and SendEhrKit pins.
- **The core pins DXVKit Plus by tag** in the workspace `Cargo.toml` (now 0.14.0 by commit, 59695e2: the Media Foundation reader, its tag to follow the Windows run; it holds `v0.13.0`, c61b1bc: MaCrate's AVFoundation track facts, plane reader and picture grab; before `v0.12.0`, 4343fe9: the encoders and the QuickTime writer; `v0.11.0`, 29fa9f5: audio tracks and real frame
  starts; C1 ran on `v0.8.0`, W0 on `v0.7.1`). Apps never
  depend on DXVKit Plus directly: one version of the decoders per app, chosen by the core.
- **One wgpu major** across DXVKit Plus, glance-core, the apps and loombda-rs (now 30.x), and the
  matching egui (now 0.36.2). A bump is made in DXVKit Plus first, then the core, then the apps,
  at most once a quarter, and only together with egui's.
- Toolchain: stable Rust, MSRV 1.95 (edition 2024). `cargo deny` (licences, bans, sources) is a
  gate since C1 (`deny.toml`, modelled on DXVKit Plus's), locally and in CI.
- **macOS builds are universal** (arm64 + x86_64; Amrit 2026-11-14: "Intel too"): two cargo
  targets and `lipo`. CI checks `x86_64-apple-darwin` from C1 and, from C2, builds the Intel
  `glance-proof` and starts it under Rosetta 2. Binaries are built for macOS 14.0 (the Swift apps'
  floor; `.cargo/config.toml`, which every app repo copies): rustc's x86_64 default (10.12) does not
  load DXVKit Plus's Apple island.
- **CI** (GitHub Actions, `.github/workflows/ci.yml`, every push): fmt, clippy, the tests, `cargo
  deny`, and `cargo check` for `x86_64-apple-darwin`, `aarch64-apple-ios` and
  `x86_64-pc-windows-msvc`. A tag is made only from a green `main`.

## 5. Waves

Each wave is one brief, ends with the gates green, a handoff, and CHECKPOINT closed. A wave that
needs something from another repo names the brief it waits for.

### Core

| Wave | Content | Done when | Waits for |
|---|---|---|---|
| **W0** (done) | recon, plan, proof | HAP + DXV play in an egui window, numbers in `03-W0-RESULTS.md` | — |
| **C1 — exact picture** (done, `v0.1.0`) | re-pin to DXVKit Plus 0.8.0 and render on its exact block path; the 0-difference render harness (CPU decode vs GPU read-back for every fixture, every variant, every size incl. 1x1 and odd sizes; scaled filter within 1); content routing and the unsupported message; `clipKey` and the alpha-look probe, identical to Glance's; Hap7 / HapH / legacy DXV play (Amrit: add, then prune); `cargo deny`; CI on GitHub. Stills moved to Glance G1 | the core's "Decode and render" and "Alpha and background" parity sections are green on Metal — `docs/04-C1-RESULTS.md` | DXVKit Plus exact path — **done** (v0.8.0) |
| **C2 — time and sound** (done, `v0.2.0`) | `glance-audio`; the audio-master clock with the 0.25 s fallback; `FrameTiming` for the OS-codec path (edit lists); loop / seek / step / scrub as in Glance; no-skip measurement on an idle machine at 60 and 120 Hz | "Time" parity section green on macOS (5 of 6 ticked; audio on Windows is C4; 60 Hz on a real display run in v0.3.3, 0 skipped) — `docs/05-C2-RESULTS.md` | DXVKit Plus: container timing, then audio — **done** (v0.10.0, v0.11.0) |
| **C3 — shell** (done, `v0.3.0`; patch `v0.3.1`: the four findings of Glance G1 — bare-key shortcuts, in-process alerts, the kit's `STAGE_DIR`, the folder listing) | `glance-platform`, `glance-settings`; the shared look (Glance's colours, spacing, fonts, icons side by side with the Swift app); transport bar, scrub bar, menus, dialogs, window-fit rules; open-file events and single window; packaging scripts (sign, notarise, DMG, `ls-cleanup`) as a reusable kit | a bundled, notarised sample app with the same bundle id as a throw-away test id reads and writes a `UserDefaults` domain and opens a file from Finder — `docs/06-C3-RESULTS.md`; the kit is `tools/package/` | — |
| **C4 — Windows milestone** | see section 7 | the core's whole parity list green on Windows 11 / D3D12 | DXVKit Plus: Windows backend (Media Foundation), after its container timing, audio and API-gap waves; Amrit's Windows PC; the signing account (section 7) |

C1–C3 are tagged `v0.1.0` … `v0.3.0`, with the patches `v0.3.1` … `v0.3.3`; Glance pins `v0.3.3`. **v0.4.0** (2026-11-17, made from CueGlance's goal run): the shell for an editor app — an app-defined menu bar (`menu::MenuBar`, `attach_menu_bar`, `ShellEvent::MenuItem`, muda re-exported), a close veto (`App::close_requested`), the quit guard (`install_terminate_guard` / `set_unsaved_work` / `PlatformEvent::QuitRequested`), the window frame saved by name (`ShellConfig::frame_autosave`), the drop's position (`Host::drop_position`), `open_path`, open / save / folder panels and a choice alert in `dialog`, and the kit's `strip -S`; CueGlance pins it. **v0.5.0** (2026-11-17, made from GlEnc's goal run): `glance-encode` (above) on DXVKit Plus v0.12.0 (its encoders and QuickTime writer), and three shell additions — `ShellEvent::DroppedAll` (every file of a drop), `dialog::open_movies` (multi-select), `glance_platform::reveal_in_finder`; GlEnc pins it (breaking only for exhaustive `ShellEvent` matches). Breaking for an app moving from v0.3.x: `ShellConfig` has a new field, `App::menu` returns `&dyn MenuBar` (`Some(&self.menu)` still compiles), matches on `ShellEvent` / `PlatformEvent` need the new variants.

DXVKit Plus's run order, which the "waits for" column follows (Cowork, 2026-11-14): exact path
(**done**, v0.8.0) → container timing (edit lists, `ctts`) → audio → API gaps (the format
description's alpha mode, the DXV variant in `MediaInfo`, and what C1 found:
`~/claude-handoffs/DXVKit-Plus_2026-11-14_c1-findings_brief-r2.md`) → Windows backend → encoders and
the QuickTime writer (approved under MIT: Amrit 2026-11-14, "Yes"; still last).

### Apps (the agreed order; each in its own repo and window)

| Wave | Content | Core needed |
|---|---|---|
| **Glance G1** | the app on macOS: window, routing, transport, folder navigation, media keys, Alpha Type, background, default-handler flow, recents | v0.3.0 |
| **Glance G2** | parity run against `docs/02-PARITY.md` (94 + 30 items), side-by-side look review with Amrit, release rehearsal under a test bundle id | |
| **Glance G3** | the swap (section 6) | |
| **Glance G4** | Windows build after C4 | v0.4.0 |
| **CueGlance Q1–Q3** | envelope canvas, cue list, undo, `.cueproj` byte-identical save, Resolume XML export; parity (95); swap. Reuses Glance's player shell from the core (in Swift it was copied) | v0.3.x |
| **GlEnc E0** | DXVKit Plus gains the encoders and the QuickTime writer, moved from loombda-rs under MIT (Amrit 2026-11-14: "Yes") — in that repo, the last wave of its run order | — |
| **GlEnc E1–E3** | `glance-encode`, the queue UI, preview, the CLI; parity (112); swap. macOS first: DXV + HAP outputs are ours, ProRes / H.264 / HEVC / MJPEG / AAC through VideoToolbox / AudioToolbox (moved from loombda-rs) | v0.5.0 |
| **MaCrate M0** | `glance-library` + the cross-test against the Swift app (`01-RESEARCH.md` §5): sidecars + stash file + `clip_id` are the iCrate contract, and the SQLite format is kept identical too; `glance-arena` (MIT, from SendEhrKit) | — |
| **MaCrate M1–M4** | scanning and watching, grid and thumbnails, search / filter / sort, metadata and sidecars, stashes, preview, drag-out, Send to Arena, Brief, measurements; parity (126); swap; iCrate check on the iPad | v0.6.0 |
| **GlancEhr** | its own W0 against the built core: macOS, Windows, and iPhone / iPad | v0.4.0 + what section 3 lists |

Windows versions of CueGlance, GlEnc and MaCrate follow each app's macOS swap, in the same order.

## 6. The replacement step (per app)

1. **Gate.** Every item of the app's checklist and the core's is ticked with evidence, or struck
   by Amrit with date and words. Verifier run on the claims.
2. **Rehearsal.** The Rust app is built, signed and notarised under a **test** bundle id
   (`com.amritus.<app>.rs`) and lives beside the Swift app; Amrit uses it for real work for a
   period he sets. No shared settings yet.
3. **Settings check.** On a copy of the real preferences (`defaults export`), the Rust app with
   the real bundle id reads every key the Swift app wrote and writes values the Swift app reads
   back (both directions, automated where the values allow). For MaCrate: the library cross-test.
4. **Swap.** The Rust app takes the Swift app's name and bundle id and the next version number in
   the same series (Glance 0.19.0, not 1.0.0, unless Amrit says otherwise); same Developer ID team,
   same document types and handler ranks; the existing `ls-cleanup` rule (exactly one
   LaunchServices record).
5. **Release** in the app's existing public releases repo: tag `v<version>`, DMG + `.sha256`, notes.
   The README of the releases repo is corrected in the same step (three of the four are stale).
6. **Roll-back stays possible.** The last Swift DMG remains in the releases repo; because settings
   and (for MaCrate) the database keep their formats, installing it again works. The Swift source
   repo is archived, not deleted, once Amrit calls the swap final.

## 7. Windows test plan

Amrit's rule: "NOTHING gets tested on ANY other computer until done." For this programme that
means: development and every proof run on the core's home machine (this Mac); Windows is touched
only at the agreed Windows milestone, on Amrit's Windows PC, with a build that already passes
everything on the Mac.

- **Windows 11 only** (Amrit 2026-11-14: "11 only"). Windows 10 is not a target: not built for,
  not tested on, and the installer refuses it with a plain sentence.
- **Before the milestone (on the Mac only):** every core crate compiles for
  `x86_64-pc-windows-msvc` in CI since C1 (`cargo check --target`, a GitHub Windows runner — a
  build machine, not a test of the product); nothing platform-specific outside `glance-platform` and
  DXVKit Plus's islands; the shaders are validated by naga for HLSL as well as MSL.
- **Milestone C4, on the Windows PC, in this order** (each step is a number or a pass / fail, not a
  look): (1) `adapter.get_info()` with `Backends::DX12` forced; BC feature present. (2) the core's
  test suite, including the 0-difference render harness on D3D12 — hard spot 7. (3) the proof:
  HAP + DXV, same clips as W0, frames shown / skipped and present intervals at the PC's refresh
  rate. (4) audio: device opens, A/V offset measured. (5) Media Foundation backend from DXVKit
  Plus: H.264 plays; HEVC presence detected and reported; ProRes reports "ProRes … is not
  supported" with the reason (decision 2: none until Apple answers). (6) shell: open-with from Explorer into the running window,
  settings location, drag-in, drag-out (`drag`), menus and accelerators (`muda`). (7) installer
  built and signed with **Microsoft Artifact Signing, Public Trust, organisation validation as
  Creativeations** (Amrit 2026-11-14: "I have a buisness, Creativeations. Registered since 2010 and
  still active."; EU organisations are eligible, individuals only in the US and Canada);
  SmartScreen behaviour on a fresh download recorded. Needed from Amrit before step 7: a business
  registration document issued within the last 12 months, the business identifier, the business
  address, and a monitored email address on a domain Creativeations owns; the certificate carries
  the registered legal name.
- **After the milestone:** each app's Windows wave repeats its own parity list on the PC. Items a
  platform cannot have (section 8) are struck for Windows in the checklist by Amrit, by name.
- The 19 "re-verify by a compile-and-run test" facts in `research/backends.md` §12.2 are assigned:
  1–5 → C1, 7–8 → C2 (already measured in DXVKit Plus for macOS), 9–11 and 16, 19 → C3, 6 →
  GlancEhr, 12–13 → C4, 14 → C2, 15 → M0, 17 → E0, 18 → C1.

## 8. Known limits, as decided (each is a decision in `01-RESEARCH.md` §9)

- ProRes on Windows: **not supported until Apple's ProRes Program Office answers** — no decode, no
  encode; the apps say so plainly where a ProRes file is opened or ProRes is offered as an output.
  Amrit writes to ProRes@apple.com as Creativeations (2026-11-14: "How do I proceed to get a yes
  from them?"). The Mac keeps Apple's own codec.
- HEVC on Windows: when the PC has the decoder. HEVC with alpha, MJPEG encode, 10-bit / 4:2:2
  H.264: Mac only.
- "Send to Arena from Finder" stays SendEhr on the Mac; an Explorer menu is a separate project.
- MaCrate's text check (Vision OCR) is Mac only: **dropped on Windows** (Amrit 2026-11-14:
  "Drop"), struck in the MaCrate checklist. A Windows library database is not interchangeable with
  a Mac one (sidecars and stash files are).
- Windows: Windows 11 only. Signing is settled (section 7): Microsoft Artifact Signing as the
  organisation Creativeations; SmartScreen still warns on a new publisher until reputation builds.
- HAP Q looks up to 2 / 255 different from Swift Glance, on purpose (the Vidvox reference;
  `docs/04-C1-RESULTS.md` §5).

## 9. Briefs written for other repos

- `~/claude-handoffs/DXVKit-Plus_2026-11-14_glance-family-needs_brief.md` — the exact shader path
  for DXT1 / DXT5 / HAP, the Windows backend, edit lists and audio, native alpha mode, the encoders
  and QuickTime writer from loombda-rs, small API gaps.
- `~/claude-handoffs/loombda-rs_2026-11-14_dxvkit-plus-dedupe_brief.md` — replace its own copies
  of the decoders with the DXVKit Plus dependency once DXVKit Plus has caught up; hand over the
  encoders.

- `~/claude-handoffs/DXVKit-Plus_2026-11-14_c1-findings_brief-r2.md` (C1) — what the core's C1 found
  in DXVKit Plus's probe and demuxer: track choice in movies with several video tracks, a HAP track
  with samples past the end of the file, content before extension, the Apple island on
  `x86_64-apple-darwin` and iOS, a lenient FourCC reader for the message.

No brief for the Swift apps (nothing in them needs to change for the rebuild; their stale docs are
listed in the research files) and none for GlancEhr (it starts after the core).
