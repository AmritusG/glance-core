# Research — what exists, with sources

2026-11-14, W0. This file is the synthesis. The full extraction, with a source line for every
fact, is in `docs/research/`:

| File | Covers | Lines | Read at commit |
|---|---|---|---|
| `research/glance.md` | Glance (Swift) + glance-releases | 972 | glance `a9dc601` (v0.18.1 + 2 doc commits) |
| `research/cueglance.md` | CueGlance + cueglance-releases | 1,103 | cueglance `66edb15` (v1.4.1) |
| `research/glenc.md` | GlEnc + glenc-public + glenc-releases | 1,473 | glenc `ed63fd6` (v1.6.2) |
| `research/macrate.md` | MaCrate, CrateKit, iCrate, SendEhrKit | 1,141 | crate `89b210c` (0.16.4), SendEhr `1a03d33` (1.1.2) |
| `research/macrate-db.md` | MaCrate's database and every other persistent format | 1,063 | same |
| `research/backends.md` | Cross-platform crates and OS APIs, from primary docs, with licences | 1,219 | web, 2026-11-14 |

Sources are written `repo/path:line`, relative to `.ref/` (read-only clones, gitignored). **CODE**
means verified in source, **DOC** means only a doc or comment says so. Where they disagree the code
wins; each research file ends with its list of disagreements (19 + 17 + 20 + MaCrate's).

Section 6 (existing Rust) and the items marked *(checked)* were read by the session that wrote this
file; the per-app files were extracted by reader agents and spot-checked — every `path:line` in
them was machine-checked to exist, and the claims that contradict the brief were re-read in source.

## 0. Contradictions with the brief (STOP items — for Amrit)

The brief's rule 7 says to stop on any contradiction. None of these blocks the W0 proof or the
core's design, so the wave was finished and each is left **undecided** here; the roadmap plans for
the code as it is and marks where a ruling changes the plan.

1. **iCrate does not share MaCrate's database.** The brief: *"iCrate: 'Shares the database' — Rust
   MaCrate must keep MaCrate's library database format so iCrate keeps working with it."* The code:
   iCrate never opens `library.sqlite`; it builds a throw-away in-memory store from the sidecar
   files on the drive each time a drive loads *(checked:
   `crate/iPad/iCrate/iCrate/SearchIndex.swift:123-125`, `MetadataStore(location: .inMemory)`)*.
   What MaCrate and iCrate share is on the **media drive**: the per-clip sidecar
   `.<clip filename>.crate-meta.json`, the per-drive `.crate-stashes.json`, and the `clip_id` hash
   (SHA-256 of the first MiB + the size). MaCrate's own comment calls the database "derived state —
   sidecars are canonical" (`crate/Sources/CrateKit/MetadataStore.swift:111-115`). So what keeps
   iCrate working is byte-compatible **sidecars**, not the SQLite format. Keeping the SQLite format
   is still worth it for a different reason (the Swift and the Rust MaCrate opening the same library
   during "Keep Swift until parity"). Detail: `research/macrate-db.md` §0, §g, §h.
2. **"HapM premultiplied vs Hap5 straight"** (the brief's example of a critical rule) is an old
   reading. Since Glance 0.18.0 both are **straight**, by Amrit's ruling of 2026-10-10 ("Keep
   straight") *(checked: `glance/CLAUDE.md:19`)*: HapM's shader premultiplies for the blend
   (`c.rgb *= c.a`), which is what the old "HapM is drawn premultiplied" comment described. GlEnc
   writes both straight.
3. **"DXV1 pads width AND height, DXV3 width only"** is still written in `glance/CLAUDE.md:470`
   but is no longer what the code does: since DXVKit took over the decode, the coded texture is
   the display size rounded up to 16 on **both** axes for every variant (`research/glance.md` §11
   item 1; DXVKit Plus says the same: "display size rounded up to 16",
   `DXVKit-Plus/docs/ARCHITECTURE.md:57`).
4. **"viewers default Premultiplied with a per-clip switch"** holds for **DXV** clips with alpha
   only. HAP alpha is shown as stated (straight, no switch); opaque clips have none.
5. **"Metal-specific shaders in the Swift apps"**: there are none on the Mac. Glance, CueGlance and
   MaCrate's preview render with **OpenGL 3.2** (GLSL 3.30 strings in
   `glance/Sources/GlancePlayback/DXVRenderer.swift`, a `CAOpenGLLayer`); the Metal renderer is
   iOS-only (iCrate) and covers DXV only. GlEnc has no GPU code at all. Porting is GLSL → WGSL.
6. **"GRDB schema/migrations"**: the 12 migrations are raw SQL, not the GRDB table DSL; no record
   stores a `Date` or `UUID`; no custom SQLite function or collation is registered. This makes a
   rusqlite twin simpler than the brief feared (`research/macrate-db.md` §0, §b, §c).
7. **Glance's Quick Look extension** does not exist on `main` (abandoned, branch
   `phase6b-experiment`), and MaCrate has no Finder extension, Quick Look, hover-scrub, "crates",
   "captures" or "recents" — "Send to Arena from Finder" is SendEhr's own Finder extension, not
   MaCrate's (`research/glance.md` §0, `research/macrate.md` §1, §10).
8. **CueGlance is not a Glance sibling with "cue features"** in the player sense: it is a
   single-window editor that makes **Resolume envelope presets** (cue points on a clip's timeline,
   exported as `ModifierEnvelope` XML). Its README still says "Phase 0 scaffold" and promises OSC /
   MIDI; neither exists in code (`research/cueglance.md` §1, §11).
9. **`<testfiles>`**, the test library every Swift repo names, is not on this Mac; a copy is
   at `<testfiles>` (12 GB) and was used
   read-only. The GlancEhr scope brief is not in `~/claude-handoffs` on this Mac.

## 1. The family

| | Glance | CueGlance | GlEnc | MaCrate | iCrate (stays Swift) |
|---|---|---|---|---|---|
| What it is | viewer / player for VJ codecs | Resolume envelope-preset editor with a clip player | queue encoder / transcoder | media library for VJ clips | iPad / iPhone companion to MaCrate |
| Bundle id | `com.amritus.glance` | `com.amritus.cueglance` | `com.amritus.glenc` | `com.amritus.crate` | `com.amritus.icrate` |
| Version read | 0.18.1 | 1.4.1 | 1.6.2 (CLI `glenc-cli` 1.0.0) | 0.16.4 | 0.15.0 |
| Min OS | macOS 14 | macOS 14 | macOS 14 | macOS 14 | iOS 17 |
| Sandbox | off | off | off | off | (iOS) |
| Depends on | DXVKit 1.0.0 (C, MIT), vendored snappy-c | Glance 0.18.0 as a package + local ports of Glance's player shell | Glance (FrameTiming), DXVKit 1.0.0 | Glance 0.18.0, GRDB 7.10.0, SendEhrKit 1.1.2 (**GPL-3.0-or-later**) | CrateKit, GlanceCore, GlancePlayback |
| Settings | 3 `UserDefaults` keys (`Glance.*`) | 4 keys (`CueGlance.*`) + the `.cueproj` sidecar per clip | 20 keys (`glenc.*`) | ~40 keys (`Crate.*`) + JSON files + SQLite in `~/Library/Application Support/com.amritus.crate/` | — |
| Release repo (public) | `AmritusG/glance-releases` | `AmritusG/cueglance-releases` | `AmritusG/glenc-releases` (+ source mirror `glenc-public`) | `AmritusG/CrateRelease` | none (Xcode install) |
| Notary keychain profile | `NotaryBuilds` | `amritus-notary` | `amritus-notary` | `amritus-notary` | — |
| Updater | none | none | none | none | — |
| Licence of own source | MIT | (private) | MIT | (private) | — |

Common release shape (all four): `swift build -c release` → hand-assembled `.app` → `codesign`
(Developer ID, hardened runtime, `SIGNING_IDENTITY` env or a default in the script) → `notarytool
--keychain-profile` → staple → DMG (`hdiutil`, UDZO, app + `/Applications` link) → sign, notarise
and staple the DMG → `.sha256` → `gh release create v<version>` on the public releases repo →
`ls-cleanup.sh` so exactly one LaunchServices record remains. Host architecture only (Apple
Silicon in practice). Versions: both plist keys equal, tag `v<version>`. Full detail per app in
§7 of each research file.

## 2. What each app does (one paragraph; the inventory is in the research file §2)

**Glance.** One window, one clip. Opens by menu, drop, or "Open with" (it offers itself as the
default for `.mov`); routes by content to its DXV player (DXVKit), its own HAP players (Hap1 / 5 /
Y / M / A), or AVFoundation for everything else; anything else shows "<codec> in <CONTAINER> is not
supported". Transport bar (play, scrub, frame step, loop, volume, mute), folder next / previous,
media keys, window fitted to the clip's aspect, background Black / Checkerboard, per-clip Alpha
Type for DXV alpha. No speed control, no in / out points, no zoom modes, no stills, no Hap7 / HapH,
no CLI arguments, no URL scheme. 91 tests.

**CueGlance.** Loads one clip in a video pane (Glance's players), shows a cue list and an envelope
canvas; cue points carry time, value 0–100 and a Resolume curve id (1–23); undo / redo; exports a
Resolume `ModifierEnvelope` XML preset to `~/Documents/Resolume Arena/Presets/Envelopes` by
default; saves a per-clip sidecar `clip.cueproj` (JSON, CueSync schema "3.0") that must re-save
byte-identically. 24 tests.

**GlEnc.** A queue of jobs: any decodable source → DXV3 (DXT1 / DXT5 / YCG6 / YG10, through
DXVKit), HAP (Hap1 / 5 / Y / M; HapA from the CLI only; its own CPU encoders), or ProRes 422 / HQ /
LT / Proxy / 4444, H.264, HEVC, MJPEG (all through `AVAssetWriter`). Trim, crop, resize, frame-rate
rules, audio pass / PCM / AAC, alpha preview, per-source Alpha Type, naming and collision rules, a
CLI. It never registers as the default handler for any type. It writes its own QuickTime container
for DXV and HAP (86-byte sample entry, one sample per chunk because Arena skews otherwise, `co64`
past 4 GB, own LPCM track).

**MaCrate.** Library roots (folders) scanned and watched (FSEvents), a thumbnail grid with
metadata under each cell, FTS search, filters and sorts, tags / rating / BPM / mood / genre /
energy, orphans and duplicates, stashes (per-drive collections), a preview player, drag-out to
other apps, Open With, Send to Arena (SendEhrKit: REST + websocket on `localhost:8080`), a "Brief"
feature (text description of a clip through the Anthropic API, key in the Keychain), and a
measurement scanner (loop quality, text detection through Vision OCR). The database is a cache;
the **sidecars on the drive are canonical**. 334 tests.

**iCrate.** Reads a drive's sidecars and stash file into an in-memory store, browses, previews
(Metal, DXV only), edits metadata back into the sidecars.

## 3. Critical rules (each one decides code in the core)

| Rule | Statement | Source |
|---|---|---|
| Exactness bar | Rendered DXV and HAP frames equal the CPU decode at 1:1, **max difference 0** per channel, every variant, every size 1x1 … 3840x2160; scaled within 1 of a software model. Reached by uploading blocks as integer textures (`RG32UI` / `RGBA32UI`, one texel per block) and decoding in the fragment shader; "Apple GPUs' fixed-function BC decode is not bit-exact" (BC4 up to 1.7, BC3 alpha up to 2). | `glance/CLAUDE.md:472,474` *(checked)*; `research/glance.md` §4, §5 |
| Alpha, readers | DXV alpha: per-clip Alpha Type, default Premultiplied ("Resolume Arena's name and default"), Straight = `rgb *= a` before a `ONE, ONE_MINUS_SRC_ALPHA` blend; the hint never changes the setting; remembered by `clipKey` (SHA-256 of byte size, frame count, size, first + last packets). HAP alpha straight. ProRes 4444 / HEVC alpha as the format description states. | `glance/CLAUDE.md:15-20,475` *(checked)* |
| Alpha, writers | Every alpha output is written straight; un-premultiply happens on read (DXV sources through the per-source Alpha Type; HEVC that states premultiplied). | `research/glenc.md` §5 |
| Coded size | DXV: display size rounded up to 16 on both axes (inside DXVKit). HAP: rounded up to 4. Subsampled planes are `ceil`, never `floor`. | `research/glance.md` §4; `glance/CLAUDE.md:473` |
| Routing | By content, never extension. DXV = sample entries `DXD3` / `DXD4` / `DXDI`, the variant from the first packet; a literal `DXV3` FourCC is not a real sample entry and is rejected. | `research/glance.md` §3, §11 item 17 |
| Checkerboard | 8 pt squares, greys 0.85 / 0.65, in layer points scaled by the display scale; thumbnails use `max(8, longer side / 10)` instead. Shared type `BackgroundMode` (`black` / `checkerboard`). | `glance/CLAUDE.md:471` *(checked)* |
| Frame timing | One rule for the OS-codec path (`GlanceCore.FrameTiming`): seek to the frame's real start from the sample table through the edit list, zero tolerance; rate as an exact rational from it. Four wrong ways are listed with the errors they cause. | `glance/CLAUDE.md:476` *(checked)* |
| Clock | DXV / HAP follow an audio master clock (the audio node's rendered sample time), wall clock after 0.25 s without audio progress, wall clock re-anchored per loop without audio; one latest-wins decode slot, no frame queue. | `research/glance.md` §4, `research/cueglance.md` §4 |
| GlEnc timebase | Integer fps → `fps × 512 / 512`; 23.976 / 29.97 / 59.94 → `N × 1000 / 1001`; any other rate fails a DXV / HAP job. | `research/glenc.md` §5 |
| GlEnc container | 86-byte sample entry, vendor `FFMP`, depth 24 even with alpha, no extension atoms, one sample per chunk, `co64` past 4 GB. | `research/glenc.md` §4 |
| GlEnc colour | The whole pipeline is 8-bit BGRA; there is no colour tagging or matrix / range handling anywhere. | `research/glenc.md` §5 |
| Single window | `Window`, not `WindowGroup`: a second file opens in the existing window. | `glance/CLAUDE.md:467` |
| No default-handler grab | GlEnc never claims to open a type (LaunchServices handler rank none). | `research/glenc.md` §2 |
| Sidecar canon | `.<clip filename>.crate-meta.json`, snake_case, sorted keys; `.crate-stashes.json`, camelCase; last writer wins by `updated_at`; unknown keys are dropped on save by both Swift apps. | `research/macrate-db.md` §h |
| Verify before fixing | "Always reproduce a claimed issue before chasing it"; "always compare against DXVKit, never only old vs new". | `glance/CLAUDE.md:463,472` |

## 4. Bundle ids, settings and the takeover

"Each Rust app replaces its Swift one under the same name and bundle id once it matches, so
settings carry over."

- All four apps are unsandboxed and keep preferences in the standard `UserDefaults` domain of
  their bundle id (`~/Library/Preferences/<bundle id>.plist`, through `cfprefsd`). A Rust app
  bundled with the same `CFBundleIdentifier` reads and writes that same domain through
  `NSUserDefaults` (objc2-foundation) — reading the plist file directly is wrong (cfprefsd caches).
  This needs one small macOS-only `unsafe` island; in keeping with rule 3 of `CLAUDE.md` it should
  live outside this workspace's `forbid` (roadmap §2, crate `glance-settings`, open point).
- Value encodings to reproduce exactly: Glance's `Glance.alphaTypeByClip` is `[String]` of
  `"<sha256>=<premultiplied|straight>"` (cap 2,000, newest last); CueGlance's is an array of
  2-string arrays — the two apps differ (`research/glance.md` §6, `research/cueglance.md` §6).
  GlEnc's 20 keys and MaCrate's keys are tabled in their §7 / §6.
- MaCrate also owns files in `~/Library/Application Support/com.amritus.crate/`: `library.sqlite`,
  `measurements.sqlite`, `library-roots.json` (holds Apple **bookmark blobs**, an opaque format a
  Rust app cannot resolve without AppKit / Foundation), a `thumbs` cache (16-hex keys), and more
  (`research/macrate-db.md` §a, §h).
- Windows has no predecessor to take over; one per-user location per app is to be chosen.
- LaunchServices: the Rust `.app` must keep the document types / handler ranks of its twin
  (Glance: viewer for `.mov`, `.mp4`, `.avi`, generic movie; GlEnc: none).

## 5. MaCrate's persistent data (full text: `research/macrate-db.md`)

- **Files.** `~/Library/Application Support/com.amritus.crate/library.sqlite`; a second database
  `measurements.sqlite` in the same folder is ATTACHed as schema `meas` on every connection.
- **Connection.** GRDB `DatabaseQueue` (never a pool), rollback journal (no WAL), foreign keys ON,
  busy timeout 5 s. No custom SQL functions or collations; no `COLLATE`, triggers or expression
  indexes in the schema (all indexes BINARY).
- **Migrations.** 12, raw SQL, `v1_initial_schema` … `v12_clip_alpha`, quoted verbatim in §c.
  Applied ids live in `grdb_migrations(identifier TEXT PRIMARY KEY)`. No
  `eraseDatabaseOnSchemaChange`. GRDB 7.10.0 ignores unknown newer identifiers, so an older Swift
  build opens a further-migrated file without complaint — a Rust build must not add migrations
  while the Swift app is still in use, or must add them in both.
- **Encodings.** Timestamps are REAL Unix seconds; ids are TEXT (stash ids uppercase UUID strings);
  two measurement columns hold ISO-8601 text; paths are absolute POSIX strings (prefix range
  queries on `/`, an unmounted-volume test hard-coded to `/Volumes/<name>`).
- **FTS.** FTS5 table `clip_search` (`unicode61`), maintained by hand (delete then insert), not by
  triggers.
- **Only in SQLite** (lost if the file is dropped): `saved_filters`, `clip_alpha` (Alpha Type
  choice + probe cache), `clips.dismissed_at`, everything in `measurements.sqlite`.
- **The iCrate contract** (section 0 item 1): sidecar JSON, stash JSON, `clip_id` hash. The
  sidecar's `version` field is never compared; the stash file has none.
- **Caveat.** The final schema in §d was derived by replaying the migration SQL in an in-memory
  SQLite; it has not yet been compared with a file the Swift app wrote. The cross-test closes that.

**Plan for the Rust crate (`glance-library`, built in the MaCrate wave, not before):**
`rusqlite` with the `bundled` feature (FTS5 on), the 12 migrations as the same SQL strings under
the same identifiers in `grdb_migrations`, the same pragmas, the hand-kept FTS; plus `sidecar` and
`stash` modules that write the JSON byte-for-byte as Foundation does (sorted keys, the `" : "`
separator and `\/` escaping where the Swift encoder produces them — to be pinned by fixtures) and
the `clip_id` hasher.

**Cross-test against the Swift app** (four directions, all automated except where noted):
1. *Swift → Rust, schema:* a database created by the Swift test suite (CrateKit's `MetadataStore`
   at a temp path) → open with rusqlite → `sqlite_master` SQL and `PRAGMA` values equal the Rust
   crate's own fresh database, statement for statement.
2. *Swift → Rust, rows:* the Swift fixture library (clips, tags, stashes, filters, alpha rows) →
   every table dumped in primary-key order equals what the Rust crate writes for the same inputs.
3. *Rust → Swift:* a database written by the Rust crate → opened by the Swift `MetadataStore` in an
   XCTest in the crate repo (a brief for that repo when the wave starts) → the same queries return
   the same rows; `grdb_migrations` is accepted; a Swift write is then read back by Rust.
4. *Sidecars and stashes:* a corpus of real `.crate-meta.json` files (one sits in the test folder:
   `.YCG6-DXV3_DXV Normal Quality.mov.crate-meta.json`) → Rust read + re-write is byte-identical;
   `clip_id` equals the Swift hasher's on the same files; and, by hand once, a drive written by the
   Rust MaCrate loads in iCrate on the iPad (Amrit's eyes).

## 6. Existing Rust (read by this session)

### DXVKit Plus 0.7.1 — `AmritusG/DXVKit-Plus`, public, MIT, edition 2024, MSRV 1.95

| Crate | What it gives the core | Source |
|---|---|---|
| `dxvkit-plus-core` | QuickTime / MP4 demux of video tracks with `tmcd` timecode (`container`); HAP decode, every variant incl. Hap7 and HapH (`hap`); DXV decode, every variant, bit-exact on DXVKit's 334 vectors (`dxv`); LMBD / LMBX; CPU BC decoders (`bc`, `bc7`); `source::probe` / `open` → `FrameSource` (`decode`, `fork`, `layout`, `decode_into`); `cache::Player` (LRU + read-ahead workers, exact frame on a miss); `transport::Transport` (exact integer time; loop, bounce, once, random, speed, in / out, seek, scrub, step, cues, beat and timecode sync); `time::{Tick, FrameRate}`; the `Backend` trait for OS decoders. `#![forbid(unsafe_code)]`, no GPU dependency. | `DXVKit-Plus/README.md`, `docs/ARCHITECTURE.md`, `crates/dxvkit-plus-core/src/{source,cache,transport,backend}.rs` |
| `dxvkit-plus-wgpu` | wgpu 30: `Gpu`, BC texture upload, `FrameRenderer` (upload → WGSL conversion → `Rgba8Unorm` as stored at clip size), staged / pipelined uploads (`Uploader`), `DirectView`, `read_texture_rgba`. DXV High Quality is decoded in the shader from integer texels, bit-exact; DXT1 / DXT5 and HAP use the GPU's BC decode (≤ 2 / 255). | `crates/dxvkit-plus-wgpu/src/{lib,convert}.rs` |
| `dxvkit-plus-apple` | macOS: AVFoundation / VideoToolbox → IOSurface → wgpu Metal texture, zero copy; frame-exact random access. The only crate with `unsafe`. | `crates/dxvkit-plus-apple/src/{decode,surface}.rs` |
| `dxvkit-plus-lmbd-enc`, `-cli` | LMBD encoders; the `dxvkit-plus` command (probe, decode, verify, bench). | |

What it does **not** have (checked in source): audio of any kind; edit lists and composition
offsets ("documented, not silently wrong", `container.rs:11`); a Windows backend (a documented
stub, `backend/windows`); HAP or DXV **encoders**; a QuickTime writer; the 0-difference shader
path for DXT1 / DXT5 / HAP; the native alpha mode of a format description; the DXV variant in
`MediaInfo` (the core reads it from the planes).

### loombda-rs — `AmritusG/loombda-rs`, private, `LicenseRef-Proprietary`

The VJ program DXVKit Plus was carved out of. Relevant crates (it has 67):

| Crate | What | Status against DXVKit Plus |
|---|---|---|
| `loombda-media` (12.8k lines) | demux, DXV, HAP, transport, cache, stills (image, exr, dds, svg, lottie) | The **ancestor** of DXVKit Plus's core, and since ahead of it in places: its `container.rs` reads edit lists and sound-track descriptions (`container.rs:148,210`). Two copies of the same decoders now exist. |
| `loombda-encode` (5.5k lines) | HAP encoders for every variant (BC1 / BC3 through the `block_compression` crate, MIT; own scaled-YCoCg and BC4 from the NVIDIA paper's prose; BC7 from the bc7enc port), a **DXV3 encoder** (safe-Rust port of DXVKit's `enc*.c` @ `fb506da`), ProRes / H.264 / HEVC through VideoToolbox, PNG / EXR sequences, **its own QuickTime muxer** with PCM and AAC audio | Exactly what GlEnc-rs needs. Not in DXVKit Plus. |
| `loombda-island-media-macos` | native decode (ancestor of `dxvkit-plus-apple`), `VtEncoder` (VideoToolbox ProRes 422 / 4444, H.264, HEVC; straight-alpha tagging), `AacEncoder` (AudioToolbox), audio read-back, waveform peaks, a Media Foundation design note | Encode + audio parts not in DXVKit Plus. |
| `loombda-audio` | cpal 0.18 output, rtrb, own resampler, own mixer and clock locked to `Tick`; decoders hound / claxon / lewton / nanomp3 | Reference for the core's audio crate. kira and firewheel were ruled out there (MPL dependency). |
| `loombda-editor` | egui 0.36 on its own winit loop (egui-winit + egui-wgpu, no eframe), previews through `register_native_texture`, immediate viewports for extra windows, monitor picking | The host pattern `glance-ui` follows. Lesson recorded there: a surface sharing the *program's* device can stall it on resize; outputs get their own device. |
| `vendor/dxvkit-plus-lmbd` | DXVKit Plus @ `80f96ee`, vendored (LMBD only) | A third copy of part of DXVKit Plus. |

### DXVKit (C) — `AmritusG/dxvkit`, public, MIT

The clean-room C library the Swift apps link (exact 1.0.0). DXVKit Plus's `core::dxv` is its
line-for-line Rust port, verified bit-exact against its vectors and its C core. The Rust
programme does not link the C library; its test vectors stay the reference.

### Decision: who owns what (no duplicate decoders across repos)

| Capability | Owner | glance-core does |
|---|---|---|
| Demux, HAP / DXV / LMBD decode, CPU BC decode, cache, transport, exact time | **DXVKit Plus** (as is) | re-exports through `glance-media` / `glance-playback` |
| BC upload, as-stored conversion, staged uploads | **DXVKit Plus** (as is) | adds only the Alpha Type pass and the viewer's background (`glance-gpu`, `glance-ui`) |
| 0-difference shader decode of DXT1 / DXT5 / HAP | **DXVKit Plus** (to build — brief) | consumes it; holds the parity test |
| macOS native decode + zero-copy import | **DXVKit Plus** `-apple` (as is) | chooses the backend |
| Windows native decode (Media Foundation + DX12 import) | **DXVKit Plus** (to build — brief; designed already) | chooses the backend |
| Edit lists / `ctts`, sound-track descriptions, audio sample access | **DXVKit Plus** (to take over from loombda-media — brief) | builds the audio clock on it |
| HAP + DXV encoders, QuickTime muxer, VideoToolbox / AudioToolbox encode | **DXVKit Plus**, new crates, moved from loombda-rs (Amrit's own code; needs his yes to release as MIT — brief) | GlEnc-rs consumes them through a thin `glance-encode` policy crate |
| Audio output, mixing, the audio-master clock | **glance-core** `glance-audio` (cpal; patterned on loombda-audio, not copied without Amrit's yes) | owns |
| egui host, look, video widget, settings, library DB, sidecars, Arena client | **glance-core** | owns |
| loombda-rs's own copies of the decoders | to be replaced by a DXVKit Plus dependency in loombda-rs (brief, low urgency) | — |

## 7. Cross-platform backends (full text, with a URL per fact: `research/backends.md`)

Versions are the current ones on 2026-11-14. Everything listed is MIT, Apache-2.0, BSD or Zlib
unless flagged.

| Need | Choice | Version | Licence | What matters |
|---|---|---|---|---|
| GPU | `wgpu` | 30.0.2 | MIT OR Apache-2.0 | Metal on macOS. On Windows a default adapter request can return **Vulkan**, not D3D12 (backend order in wgpu-core); the apps set `Backends::DX12` on Windows, which any Media Foundation interop needs anyway. A new major about every three months; the pin must equal egui-wgpu's. |
| BC textures | `Features::TEXTURE_COMPRESSION_BC` | — | — | Always on under Metal on macOS (every Apple silicon Mac) and under D3D12; Vulkan only if the device reports it. BC1 / BC3 / BC4 / BC5 / BC6H / BC7 formats all exist. **BC textures must be multiples of 4** in wgpu 30 (no `texture-compression-unaligned` yet): odd sizes use a padded texture and a crop — which DXVKit Plus already does. |
| UI | `egui`, `egui-wgpu`, `egui-winit` | pinned **0.36.2** (0.37.0 is out since 2026-10-15) | MIT OR Apache-2.0 | Both need wgpu ^30 and winit ^0.30.13. 0.36.2 is what loombda-rs runs and what W0 proved; moving to 0.37 is a deliberate bump with loombda-rs (roadmap §4). egui-wgpu dithers by default — off for video. |
| Windowing | `winit` | 0.30.13 | Apache-2.0 | No "open file" event on macOS: the app installs its own `NSApplicationDelegate` (`application:openURLs:`), which winit documents as supported; bundled builds only. Borderless fullscreen on a chosen monitor is supported. |
| Second-screen output | an app-owned winit window + wgpu surface | — | — | egui viewports repaint with the UI; loombda-rs measured that an output must not share the UI's pacing or device (`loombda-rs/CLAUDE.md` law 4). Needed by GlancEhr, not by the four apps. |
| File dialogs / menus | `rfd` 0.18.0 (MIT) / `muda` 0.21.2 (Apache-2.0 OR MIT) | | | muda's accelerators on Windows need the message loop to call `TranslateAcceleratorW` — unverified under our own winit loop. |
| Drag **out** to other apps | `drag` | 2.1.1 | Apache-2.0 OR MIT | The only crate found (CrabNebula). Works from a raw window handle on macOS and Windows; unverified with winit 0.30. MaCrate's most-used feature depends on it. |
| macOS decode | `objc2` 0.6.5 + framework crates 0.3.2 | | MIT / Zlib OR Apache-2.0 OR MIT | Already built and measured in `dxvkit-plus-apple` (zero copy). |
| Windows decode | `windows` crate, Media Foundation Source Reader | 0.62.2 | MIT OR Apache-2.0 | H.264 and AAC in-box. HEVC: the decoder exists but may be absent on a given PC (Store package) — detect with `MFTEnumEx`. ProRes: **no system decoder found.** The MF `.mov` source handles `avc1` / AAC / 8- and 16-bit PCM; edit lists, `hvc1` in `.mov`, 24-bit PCM unconfirmed. Zero-copy from MF into a wgpu D3D12 texture is unproven. |
| HAP | own (DXVKit Plus), `snap` 1.1.2 (BSD-3) | | | The Vidvox spec (BSD-2 repository) is silent on alpha convention, colour space and odd sizes. |
| MOV demux / mux | own (DXVKit Plus; loombda-encode's writer) | | | `mp4parse` and `symphonia` are **MPL-2.0** (flagged, not used). The MF sink writes MP4, not MOV: HAP / DXV `.mov` on Windows needs our own writer. |
| Audio out | `cpal` | 0.18.2 | Apache-2.0 | CoreAudio / WASAPI. Its README states macOS 14.2 as the minimum for the CoreAudio backend (the Swift apps say 14.0) — to verify. Never enable its `realtime` feature (MPL dependency, per loombda-rs). |
| Audio decode | PCM by hand; AAC through the OS (AudioToolbox / Media Foundation) | | | `symphonia` is MPL-2.0. |
| SQLite | `rusqlite` + `bundled` | 0.42.0 (SQLite 3.54.1) | MIT | FTS5 in the bundled build. |
| HTTP / websocket | `ureq` 3.5.2 / `tungstenite` 0.30.0 | | MIT OR Apache-2.0 | Blocking, small; Arena is on `localhost:8080`. |
| BC encode (CPU) | `block_compression`, `intel_tex_2`, own (loombda-encode) | 0.11.0 / 0.5.0 | MIT / MIT OR Apache-2.0 | loombda-encode already encodes every HAP variant with permissive code. |
| macOS packaging | manual `.app` + `codesign` + `notarytool` | | | The existing scripts' shape carries over; a universal build is two cargo targets + `lipo`. |
| Windows packaging | `cargo-wix` 0.3.9 / `cargo-packager` 0.11.8 / Velopack 1.2.161 | | MIT OR Apache-2.0 / MIT | File associations are written by the installer; a second double-click starts a second process unless the app forwards to the first. |

19 facts are listed for a compile-and-run check in `research/backends.md` §12.2; the roadmap
assigns each to the wave that depends on it.

## 8. Hard spots, stated plainly

1. **ProRes on Windows.** macOS decodes and encodes ProRes with Apple's own codec. Windows has no
   system ProRes decoder that the research could find and no Apple encoder; Apple's only documented
   route is its ProRes Program Office (terms not public) and it calls every other implementation
   "unauthorized". SMPTE RDD 36 specifies the **decoder** only, grants no patent or trademark
   licence, and its reference source is evaluation-only (must not be read for a clean-room
   implementation). Four young Rust crates claim clean-room ProRes (the oldest seven months).
   Options: (a) no ProRes on Windows — decode and encode both — and say so in the app; (b) ask
   Apple; (c) own clean-room decoder from the RDD 36 PDF (decode only; real engineering, no licence
   grant); (c') depend on one of the young crates; (d) transcode-on-import to HAP on the Mac.
   Affects: Glance, CueGlance, MaCrate (playback of ProRes sources), GlEnc (ProRes as a source and
   as an output).
2. **Other codecs Windows lacks or may lack.** HEVC may be absent (run-time check, tell the user).
   HEVC with alpha: no Windows decoder. MJPEG decode exists; MJPEG **encode** does not. H.264 4:2:2
   / 10-bit: no. `.mov` writing for H.264 / HEVC: the MF sink writes MP4 only — our own muxer is
   needed for `.mov` outputs.
3. **GlEnc's AVFoundation half.** ProRes / H.264 / HEVC / MJPEG / AAC outputs all go through
   `AVAssetWriter`, and all non-DXV / HAP source decode, audio decode and resampling go through
   AVFoundation. On Windows: H.264, HEVC (Main), AAC through Media Foundation; ProRes and MJPEG
   outputs have no native path. DXV and HAP — the outputs VJs use — are fully ours on both.
4. **Finder-only and Mac-only features.** "Send to Arena from Finder" is SendEhr's Finder extension
   and stays on the Mac; an Explorer equivalent (a shell context-menu handler) is a separate
   project. In MaCrate: FSEvents (→ `ReadDirectoryChangesW`), bookmark blobs in
   `library-roots.json` (opaque Apple data — a one-time import needs a small Mac helper, or the
   user re-adds folders), dot-file hiding (Windows needs the hidden attribute too, same names),
   drag-out (hard spot 5), Open With / reveal (shell APIs), Keychain (→ Credential Manager),
   Vision OCR in the measurement scanner (no Windows drop-in: the text check cannot be ported
   as-is), a Windows library database is not interchangeable with a Mac one (absolute paths) while
   sidecars and stash files are.
5. **Drag-out from an egui window** exists only as the `drag` crate, unverified on winit 0.30.
6. **"Open with" into a running single window.** macOS: own `NSApplicationDelegate`. Windows:
   single-instance lock + hand-off of the path; and Windows 10 / 11 do not let an app make itself
   the default — Glance's "Set as default for .mov" becomes "open Settings".
7. **The exactness bar on every backend.** Integer textures, `textureLoad`, 32-bit shifts and
   integer division must give the same bits through naga's MSL and HLSL output on every GPU;
   proven per backend, not assumed.
8. **Sample-accurate audio clock** on CoreAudio and WASAPI through cpal (stream timestamps differ
   in meaning per backend).
9. **Windows code signing.** Private keys must sit on hardware (token or cloud HSM) for every
   certificate since June 2023; certificates last at most 460 days. Microsoft's Artifact Signing
   (USD 9.99 / month) issues Public Trust certificates to **individuals in the US and Canada
   only**, and to **organisations** in the EU, UK and a list of other countries. SmartScreen warns
   on a new publisher until reputation builds ("several weeks and hundreds of clean installs"); EV
   no longer skips that.
10. **Licences found on the way.** SendEhrKit is GPL-3.0-or-later (Amrit's own code): a Rust Arena
    client translated from it inherits that unless he relicenses it. loombda-rs is proprietary
    (his own): moving its encoders into MIT DXVKit Plus needs his yes. GlEnc's BC1 encoder is a
    port of FFmpeg's `texturedspenc` (that file is MIT-licensed inside FFmpeg, and the port is
    marked MIT) — legal, but the Rust GlEnc should use loombda-encode's encoders, which have no
    FFmpeg ancestry at all.
11. **GlEnc quirks that parity would freeze in** (code, not docs): a trimmed job muxes the
    full-length source audio; the open-with path skips the movie-type filter; "Reset to Defaults"
    leaves `checkerboardScope` alone; preview FourCC sets lack `DXD4`. Bugs or behaviour — Amrit's
    call per item (`research/glenc.md` §12).

## 9. Decisions for Amrit (one line each)

1. iCrate compatibility means sidecars + stash file + `clip_id`, not the SQLite file — confirm, and
   say whether the Rust MaCrate must also keep the SQLite format (recommended: yes, for the swap).
2. ProRes on Windows: none (a), ask Apple (b), own decoder (c), a young crate (c'), or
   Mac-only (d)? Recommended: (a) for the first Windows releases, with (b) asked in parallel.
3. The 0-difference render bar: keep it as the gate for the Rust apps (DXVKit Plus then gets the
   integer-texel path for DXT1 / DXT5 / HAP), or accept the GPU's BC decode (≤ 2 / 255, what
   Arena itself shows)? Recommended: keep it.
4. May loombda-rs's encoders, QuickTime writer and VideoToolbox / AudioToolbox encode move into
   DXVKit Plus under MIT? (They are what GlEnc-rs needs.)
5. May the Rust Arena client be written from SendEhrKit and released under the core's licence
   (relicensing your GPL code), or does it stay GPL in its own crate?
6. Windows signing: an organisation (company) for Artifact Signing, or a CA certificate on a token?
7. glance-core's own licence: proprietary and private as now, or MIT like DXVKit Plus and Glance?
8. egui 0.36.2 (as loombda-rs) or 0.37.0 for the family? Recommended: stay on 0.36.2, bump with
   loombda-rs.
9. Intel Macs: the Swift apps build for the host only (Apple silicon in practice). Universal
   builds for the Rust apps, or Apple silicon only?
10. Minimum macOS: stay at 14.0, or 14.2 if cpal needs it?
11. Windows minimum: Windows 10 or Windows 11 only?
12. GlEnc's four code quirks (hard spot 11): keep or fix in the Rust app?
13. CueGlance: the `.cueproj` writer drops `tracks`, `playlists`, `selectedTrackId` from CueSync
    files on save although a comment promises they survive — reproduce the loss, or preserve them?
14. Glance: should Play at the end of a non-looping DXV / HAP clip restart from frame 0 like the
    AVFoundation path does?
15. MaCrate on Windows: is the measurement scanner's text check (Vision OCR) dropped there?
16. Where is the GlancEhr scope brief? It is not in `~/claude-handoffs` on this Mac, so the core's
    plan covers GlancEhr's needs only as the W0 brief names them.
