# Roadmap — the Glance family in Rust

2026-11-14, W0. Amrit's decisions this plan is built on (verbatim in the W0 brief): egui
everywhere, each app keeping its current look; one shared core repo plus one repo per app; the
Swift apps stay live until their Rust twin reaches parity, then the Rust app takes over the same
name and bundle id; order core → Glance → CueGlance → GlEnc → MaCrate; GlancEhr starts after the
core is built; iCrate stays Swift; no FFmpeg.

Open decisions that change this plan are in `docs/01-RESEARCH.md` §9; where one bites, the wave
below says so.

## 1. Repos

All under `<workspace>/`, each its own Claude Code window, each private
on GitHub until Amrit says otherwise.

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
| `glance-media` | **built (W0)**, thin | probe / open, platform backend choice, codec names, the Alpha Type rule; later: content routing with the "<codec> in <CONTAINER> is not supported" message, `clipKey`, the alpha-look probe, `FrameTiming` | `dxvkit-plus-core`, `dxvkit-plus-apple` (macOS), DXVKit Plus's Windows backend when it exists |
| `glance-gpu` | **built (W0)**, thin | device, a clip's GPU state, the Alpha Type pass; later: the 0-difference render test harness, thumbnails (CPU and GPU), scaled-filter rules | `dxvkit-plus-wgpu`, `wgpu` |
| `glance-playback` | **built (W0)**, thin | a clip on the host clock, timing statistics; later: the audio-master clock, loop / scrub / step / seek semantics as the Swift apps have them, end-of-clip rules | `glance-media`, `glance-audio` |
| `glance-ui` | **built (W0)**, thin | the egui host (winit + egui-winit + egui-wgpu), the video widget, the checkerboard; later: the shared Glance look (colours, spacing, fonts, icons), transport bar, scrub bar, menus (`muda`), dialogs (`rfd`), window-fit rules, a second-screen output window (for GlancEhr) | `glance-gpu`, `egui` |
| `glance-audio` | planned (core wave C2) | device output (cpal), PCM from `.mov`, AAC through the OS, resampling, volume / mute, the position the video clock follows | `cpal`, DXVKit Plus (audio demux + OS decode — brief) |
| `glance-platform` | planned (C3) | the one island with `unsafe`: macOS preferences domain (`NSUserDefaults`), open-file events (`NSApplicationDelegate`), default-handler registration, media keys, drag-out; Windows: single instance + file hand-off, registry settings location, media transport controls | `objc2*`, `windows` |
| `glance-settings` | planned (C3) | typed settings over `glance-platform`'s store, with each app's existing keys and encodings | `glance-platform` |
| `glance-encode` | planned (GlEnc wave) | GlEnc's policy: job model, naming, trim / crop / resize, frame-rate rules, alpha rules on write | DXVKit Plus encoder crates (brief) |
| `glance-library` | planned (MaCrate wave) | MaCrate's SQLite twin (rusqlite, the 12 migrations), sidecars, stash files, `clip_id`, FTS, the folder watcher | `rusqlite` |
| `glance-arena` | planned (MaCrate wave; GlancEhr needs it too) | Resolume Arena REST + websocket client, modes, the Send To menu model | `ureq`, `tungstenite` (licence decision 5) |
| `glance-proof` | built (W0) | the W0 proof; later the core's own bench / verify binary | all |

What the core never holds: a decoder, a demuxer, an encoder, a QuickTime writer, OS-codec FFI.
Those are DXVKit Plus's (`docs/01-RESEARCH.md` §6, "who owns what").

Layering (dependencies point down only): `glance-proof` / apps → `glance-ui` → `glance-gpu`,
`glance-playback`, `glance-audio`, `glance-settings` → `glance-media`, `glance-platform` → DXVKit
Plus.

## 3. What the core must cover for GlancEhr (from the W0 brief; its scope brief was not found)

| GlancEhr need | Where it lands |
|---|---|
| Random playback | DXVKit Plus's transport already has random-frame and random-direction modes; `glance-playback` exposes them, plus a playlist layer for "next clip at random" |
| Automated transitions | a two-clip mix pass in `glance-gpu` (two `ClipView`s, one blend uniform), driven by `glance-playback` |
| Second-screen Send To | an app-owned output window on its **own** wgpu surface and pacing in `glance-ui` (never an egui viewport; loombda-rs law 4), borderless fullscreen on a chosen monitor; "Send To" Arena is `glance-arena` |
| Windows from day one | every core crate builds and passes its tests on Windows from the Windows milestone on (section 7); nothing Mac-only outside `glance-platform` and DXVKit Plus's Apple island |

When GlancEhr's own W0 runs it reads this file and `docs/01-RESEARCH.md`; anything it needs that
is missing comes back as a brief to this repo.

## 4. Versions and pins

- **glance-core** is versioned as one workspace: `0.MINOR.PATCH`, tag `vX.Y.Z` on `main`. A minor
  bump may break the API; a patch never does. Until 1.0 every app wave names the core tag it needs.
- **Apps pin the core by git tag**, never a branch, never a path:
  `glance-ui = { git = "https://github.com/AmritusG/glance-core", tag = "v0.3.0" }` (all core
  crates at the same tag). `Cargo.lock` is committed in every app repo. A core change an app needs
  is made in this repo, tagged, then the app bumps its pin in its own commit — the same discipline
  the Swift apps use for Glance and SendEhrKit pins.
- **The core pins DXVKit Plus by tag** in the workspace `Cargo.toml` (now `v0.7.1`). Apps never
  depend on DXVKit Plus directly: one version of the decoders per app, chosen by the core.
- **One wgpu major** across DXVKit Plus, glance-core, the apps and loombda-rs (now 30.x), and the
  matching egui (now 0.36.2). A bump is made in DXVKit Plus first, then the core, then the apps,
  at most once a quarter, and only together with egui's.
- Toolchain: stable Rust, MSRV 1.95 (edition 2024). `cargo deny` (licences, bans, sources) is added
  to the gates in wave C1 with DXVKit Plus's `deny.toml` as the model.

## 5. Waves

Each wave is one brief, ends with the gates green, a handoff, and CHECKPOINT closed. A wave that
needs something from another repo names the brief it waits for.

### Core

| Wave | Content | Done when | Waits for |
|---|---|---|---|
| **W0** (this one) | recon, plan, proof | HAP + DXV play in an egui window, numbers in `03-W0-RESULTS.md` | — |
| **C1 — exact picture** | content routing and the unsupported message; `clipKey` and the alpha-look probe, byte-identical to Glance's; the 0-difference render harness (CPU decode vs GPU read-back for every fixture, every size incl. 1x1 and odd sizes; scaled filter within 1); Hap7 / HapH / legacy DXV / stills behaviour decided; `cargo deny`; CI on GitHub (macOS runner) | the core's "Decode and render" and "Alpha and background" parity sections are green on Metal | DXVKit Plus brief item 1 (integer-texel path) — or decision 3 if Amrit relaxes the bar |
| **C2 — time and sound** | `glance-audio`; the audio-master clock with the 0.25 s fallback; `FrameTiming` for the OS-codec path (edit lists); loop / seek / step / scrub as in Glance; no-skip measurement on an idle machine at 60 and 120 Hz | "Time" parity section green on macOS; A/V offset measured | DXVKit Plus brief items 3–4 (edit lists, audio) |
| **C3 — shell** | `glance-platform`, `glance-settings`; the shared look (Glance's colours, spacing, fonts, icons side by side with the Swift app); transport bar, scrub bar, menus, dialogs, window-fit rules; open-file events and single window; packaging scripts (sign, notarise, DMG, `ls-cleanup`) as a reusable kit | a bundled, notarised sample app with the same bundle id as a throw-away test id reads and writes a `UserDefaults` domain and opens a file from Finder | — |
| **C4 — Windows milestone** | see section 7 | the core's whole parity list green on Windows / D3D12 | DXVKit Plus brief item 2 (Media Foundation backend); Amrit's Windows PC |

C1–C3 are tagged `v0.1.0` … `v0.3.0`; Glance can start on `v0.3.0`.

### Apps (the agreed order; each in its own repo and window)

| Wave | Content | Core needed |
|---|---|---|
| **Glance G1** | the app on macOS: window, routing, transport, folder navigation, media keys, Alpha Type, background, default-handler flow, recents | v0.3.0 |
| **Glance G2** | parity run against `docs/02-PARITY.md` (94 + 30 items), side-by-side look review with Amrit, release rehearsal under a test bundle id | |
| **Glance G3** | the swap (section 6) | |
| **Glance G4** | Windows build after C4 | v0.4.0 |
| **CueGlance Q1–Q3** | envelope canvas, cue list, undo, `.cueproj` byte-identical save, Resolume XML export; parity (95); swap. Reuses Glance's player shell from the core (in Swift it was copied) | v0.3.x |
| **GlEnc E0** | DXVKit Plus gains the encoders and the QuickTime writer (brief item 5) — in that repo | — |
| **GlEnc E1–E3** | `glance-encode`, the queue UI, preview, the CLI; parity (112); swap. macOS first: DXV + HAP outputs are ours, ProRes / H.264 / HEVC / MJPEG / AAC through VideoToolbox / AudioToolbox (moved from loombda-rs) | v0.5.0 |
| **MaCrate M0** | `glance-library` + the cross-test against the Swift app (`01-RESEARCH.md` §5); `glance-arena` | decisions 1, 5 |
| **MaCrate M1–M4** | scanning and watching, grid and thumbnails, search / filter / sort, metadata and sidecars, stashes, preview, drag-out, Send to Arena, Brief, measurements; parity (126); swap; iCrate check on the iPad | v0.6.0 |
| **GlancEhr** | its own W0 against the built core | v0.4.0 + what section 3 lists |

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

- **Before the milestone (on the Mac only):** every core crate compiles for
  `x86_64-pc-windows-msvc` in CI (`cargo check --target`, a GitHub Windows runner — a build
  machine, not a test of the product); nothing platform-specific outside `glance-platform` and
  DXVKit Plus's islands; the shaders are validated by naga for HLSL as well as MSL.
- **Milestone C4, on the Windows PC, in this order** (each step is a number or a pass / fail, not a
  look): (1) `adapter.get_info()` with `Backends::DX12` forced; BC feature present. (2) the core's
  test suite, including the 0-difference render harness on D3D12 — hard spot 7. (3) the proof:
  HAP + DXV, same clips as W0, frames shown / skipped and present intervals at the PC's refresh
  rate. (4) audio: device opens, A/V offset measured. (5) Media Foundation backend from DXVKit
  Plus: H.264 plays; HEVC presence detected and reported; ProRes reports "not supported on
  Windows" (or what decision 2 says). (6) shell: open-with from Explorer into the running window,
  settings location, drag-in, drag-out (`drag`), menus and accelerators (`muda`). (7) installer
  built and signed per decision 6; SmartScreen behaviour on a fresh download recorded.
- **After the milestone:** each app's Windows wave repeats its own parity list on the PC. Items a
  platform cannot have (section 8) are struck for Windows in the checklist by Amrit, by name.
- The 19 "re-verify by a compile-and-run test" facts in `research/backends.md` §12.2 are assigned:
  1–5 → C1, 7–8 → C2 (already measured in DXVKit Plus for macOS), 9–11 and 16, 19 → C3, 6 →
  GlancEhr, 12–13 → C4, 14 → C2, 15 → M0, 17 → E0, 18 → C1.

## 8. Known limits, as planned today (each is a decision in `01-RESEARCH.md` §9)

- ProRes on Windows: none in the first Windows releases — no decode, no encode (decision 2).
- HEVC on Windows: when the PC has the decoder. HEVC with alpha, MJPEG encode, 10-bit / 4:2:2
  H.264: Mac only.
- "Send to Arena from Finder" stays SendEhr on the Mac; an Explorer menu is a separate project.
- MaCrate's text check (Vision OCR) is Mac only; a Windows library database is not
  interchangeable with a Mac one (sidecars and stash files are).
- Windows signing route open (decision 6): individuals outside the US and Canada cannot use
  Microsoft's Artifact Signing; an organisation in the EU can.

## 9. Briefs written in W0 for other repos

- `~/claude-handoffs/DXVKit-Plus_2026-11-14_glance-family-needs_brief.md` — the exact shader path
  for DXT1 / DXT5 / HAP, the Windows backend, edit lists and audio, native alpha mode, the encoders
  and QuickTime writer from loombda-rs, small API gaps.
- `~/claude-handoffs/loombda-rs_2026-11-14_dxvkit-plus-dedupe_brief.md` — replace its own copies
  of the decoders with the DXVKit Plus dependency once DXVKit Plus has caught up; hand over the
  encoders.

No brief for the Swift apps (nothing in them needs to change for the rebuild; their stale docs are
listed in the research files) and none for GlancEhr (it starts after the core).
