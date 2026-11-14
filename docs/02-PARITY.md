# Parity checklists

The gate each Rust app must pass before it replaces its Swift twin under the same name and bundle
id ("Keep Swift until parity"). One checklist per app, plus the core's own, which every app
inherits.

How to use it:
- Every item is judged against the **Swift source** as read on 2026-11-14 (commits in
  `docs/01-RESEARCH.md` §1), not against its docs. The detail and the source line for each item are
  in `docs/research/<app>.md`; section references (`§2.1`, `2.6`) point there.
- An item is ticked only with evidence (a test name, a measured number, or Amrit's eyes for looks).
  Items Amrit decides to drop or change are struck with the date and his words, never silently removed.
- Platform wording: shortcuts are written the Mac way (⌘); on Windows ⌘ is Ctrl unless an item says
  otherwise. Items marked macOS-only have no Windows gate.
- The numbers in the items (sizes, limits, timings) are the Swift apps' and are the pass criteria.

Counts: core 30, Glance 94, CueGlance 95, GlEnc 112, MaCrate 126.

## Core (glance-core + DXVKit Plus) — inherited by every app

### Decode and render
- [ ] Routing is by file content, never by extension: `DXD3` / `DXD4` / `DXDI` sample entries → DXV (the variant from the first packet); `Hap1` / `Hap5` / `HapY` / `HapM` / `HapA` → HAP; everything else → the OS decoder; what nothing decodes shows "<codec> in <CONTAINER> is not supported" (no fallback engine).
- [ ] DXV3 DXT1, DXT5, YCG6, YG10 and legacy DXV1 / 2 (`DXDI`, raw and LZF) decode bit-exact with DXVKit's test vectors (DXVKit Plus: 334 rows).
- [ ] The DXV coded texture is the display size rounded up to 16 on **both** axes for every variant; HAP pads to 4. (The older "DXV3 pads width only" rule is gone from the Swift code.)
- [ ] **The exactness bar:** every DXV and HAP variant renders at 1:1 with max difference 0 per channel against the CPU decode (DXVKit's RGBA; the CPU HAP decoder), at every size from 1x1 to 3840x2160 including odd and non-16 sizes. Blocks are decoded in the shader from integer textures, never by the GPU's BC decoder. *W0: met for DXV High Quality only; 1–2 / 255 measured for DXT1 / DXT5 / HAP (DXVKit Plus brief).*
- [ ] Window-scaled rendering (1.5x, 0.5x) is within 1 of a software model of the same filter.
- [ ] The bar holds on every backend the apps ship on: Metal (macOS), Direct3D 12 (Windows); Vulkan if it is ever enabled.
- [ ] Subsampled planes cover every pixel (ceil, never floor): a 1-row clip has a 1-row chroma plane.
- [ ] Hostile files (truncated, garbage, wrong FourCC for the payload, zero bytes) give an error message, never a crash or a hang.

### Alpha and background
- [ ] Frames are decoded as stored; nothing un-premultiplies or premultiplies on decode.
- [ ] DXV clips with alpha (DXT5, YG10, legacy DXT5) get a per-clip "Alpha Type": Premultiplied (default) / Straight. Straight multiplies colour by alpha in the present pass; the blend is `ONE, ONE_MINUS_SRC_ALPHA`.
- [ ] HAP alpha (Hap5, HapM) is straight, shown as stated ("Alpha Type: Straight (HAP)", no switch). Native codecs that state their alpha mode (ProRes 4444, HEVC with alpha) follow the format description.
- [ ] The Alpha Type hint ("looks premultiplied" / "looks straight" / "can't tell") uses Glance's probe: first + middle frame, share of pixels with 8 < alpha < 200 whose max(R,G,B) > alpha + 24; ≤ 0.5 % premultiplied, ≥ 3 % straight, else or < 500 samples can't tell. The hint never changes the setting.
- [ ] A clip's choice is remembered by content, not path: `clipKey` = SHA-256 of byte size, frame count, size and first + last packets, byte-identical to Glance's, so choices made in the Swift apps still apply.
- [ ] Checkerboard: 8 pt squares, greys 0.85 and 0.65, in window points scaled by the display's scale factor; Black is the other mode; opaque clips look the same in both.
- [ ] Writers write straight alpha (GlEnc, DXV and HAP).

### Time
- [ ] Frame `n` is exactly frame `n` after any seek, on every decode path (shuffled seeks equal sequential decode).
- [ ] The OS-decoder path takes each frame's real start from the sample table through the edit list (Glance's `FrameTiming` rule): never seconds rounded to 1/600, never the N / fps grid alone, never the shortest sample duration as the rate.
- [ ] Rates are exact rationals end to end (23.976 = 24000/1001); no float accumulates in the playhead.
- [ ] DXV and HAP playback follows the audio clock when the clip has audio (frame = nearest to the audio position), falling back to the wall clock after 0.25 s without audio progress; without audio, a wall clock re-anchored each loop.
- [ ] Audio in `.mov` plays for at least PCM and AAC on both OSes; an undecodable audio track plays the picture silently and says so.
- [ ] No clip frame is skipped at 1x on a 60 Hz or 120 Hz display for 1080p and 2160p HAP and DXV on the reference machines (W0: 0 skipped, `docs/03-W0-RESULTS.md`).

### Settings, identity, release
- [ ] Each Rust app carries its Swift twin's bundle id (`com.amritus.glance`, `com.amritus.cueglance`, `com.amritus.glenc`, `com.amritus.crate`) and reads and writes the same preference keys in the same store on macOS (the `UserDefaults` domain of that id), with the same value encodings, so settings survive the swap in both directions.
- [ ] On Windows the same keys live in one documented per-user location per app.
- [ ] macOS: Developer ID signed, hardened runtime, notarised, stapled, DMG; the release goes to the app's existing public releases repo as `v<version>` with the DMG and its `.sha256`; exactly one LaunchServices record after install.
- [ ] Windows: signed installer; SmartScreen does not block a fresh download once reputation exists (the signing route is Amrit's decision, `docs/00-ROADMAP.md` §8).
- [ ] No FFmpeg, libav, GStreamer or libmpv is linked or bundled; every dependency's licence is MIT / BSD / Apache-2.0 / Zlib / ISC (or approved by Amrit by name), enforced by `cargo deny` in every app repo.
- [ ] No crash on any input: the never-crash lints hold in every crate; a GPU without BC texture support gets a clear message.
- [ ] The app's look matches the Swift app side by side (Amrit's eyes): layout, spacing, colours, fonts, icons — "keep each app's current look".
- [ ] Menus, shortcuts and wording match the Swift app item for item unless an item in the app's list says otherwise.
- [ ] Each app's own test suite covers its list; the Swift app's exactness and timing fixtures are reused where the licence allows.


## Glance

Detail and sources: `docs/research/glance.md`.

### Identity and packaging
- [ ] App is named `Glance`; version string shown to the OS equals the release version in every version field.
- [ ] macOS build runs on macOS 14 or later.
- [ ] One process, one window: opening a second file while running reuses the window and never opens another.
- [ ] Quitting: closing the window quits the app.

### Opening files
- [ ] File ▸ `Open…` (⌘O / Ctrl+O) opens one movie file; folders cannot be chosen.
- [ ] Dropping files on the video area opens the first one; any file type is accepted and routed.
- [ ] A file passed by the OS ("Open with", double-click, drop on the app icon) opens in the existing window; with several files the last one wins.
- [ ] The OS file association covers `.mov`, `.mp4` / MPEG-4, `.avi` and generic movie types, role viewer.
- [ ] Menu item `Set Glance as Default for .mov Files…` exists and reports success, "already the default", or failure with manual instructions.
- [ ] First launch asks once whether to become the default for `.mov`; never asks again after an answer, or if already the default.
- [ ] Opened files are added to the OS recent-documents list.
- [ ] Opening a new file while the previous one is still loading shows only the new one.

### Routing and formats
- [ ] Routing order: DXV → HAP → system codecs → "not supported" message.
- [ ] DXV3 DXT1, DXT5, YCG6, YG10 play.
- [ ] Legacy DXV1 / DXV2 (`DXDI`) DXT1 and DXT5, RAW and LZF, play.
- [ ] `DXD4` sample entries open; a legacy packet inside a `DXD3` track decodes as legacy; DXV3 version byte < 4 and stored flag > 1 are refused; sample-entry FourCCs `DXT1` / `YCG6` are refused.
- [ ] Stored (uncompressed) DXV packets decode.
- [ ] A mid-file change of DXV texture format is reported as a decode error and playback continues with the next frame.
- [ ] `Hap1`, `Hap5`, `HapY`, `HapM`, `HapA` play.
- [ ] HAP sections `0xAB/BB/CB`, `0xAE/BE/CE`, `0xAF/BF/CF`, `0xA1/B1/C1` and the `0x0D` container decode; 4-byte and 8-byte section headers both work.
- [ ] Chunked HAP sections with 1 and with ≥ 2 chunks, mixing `0x0A` and `0x0B` chunks, decode.
- [ ] HapM with colour and alpha sections in either order plays; a HapM frame missing either section is a decode error for playback.
- [ ] The HAP texture kind is taken per frame from the section type.
- [ ] H.264, HEVC (incl. alpha), ProRes 422 family and 4444 (incl. alpha), Motion JPEG play with audio.
- [ ] Unplayable files show `<codec> in <CONTAINER> is not supported` (e.g. `VP9 in WEBM is not supported`, `HEVC in MKV is not supported`, `QuickTime Animation in MOV is not supported`) or `<CONTAINER> files are not supported`, with the caption line, instead of a black window.
- [ ] A DXV or HAP file whose player cannot start shows `This DXV file could not be opened` / `This HAP file could not be opened`.

### Pixel exactness
- [ ] At 1:1 with blend off, every DXV variant renders equal to DXVKit's CPU RGBA: max difference 0 on R, G, B and A, interior and edge.
- [ ] That holds at 1×1, 17×17, 18×10, 30×30, 333×221, 908×2276, 1080×1080, 1281×719, 1284×724, 1920×1080, 3840×2160.
- [ ] At 1:1 every HAP variant renders equal to the CPU HAP decoder: max difference 0.
- [ ] Window-scaled (1.5× and 0.5×) output is within 1 level of a software model (bilinear over decoded texels; HQ chroma nearest).
- [ ] BC blocks are decoded in the shader from integer textures; no GPU BC texture format is used.
- [ ] BC1: 5:6:5 bit-replicated, thirds and halves truncated; three-colour index 3 is black with alpha 255; DXT5 colour blocks always four-colour.
- [ ] BC4: truncated 8-bit palette for DXT5 alpha / HapY luma / HapM alpha; exact 1/35 palette for DXV HQ planes.
- [ ] DXV HQ colour: chroma midpoint 127.5, non-reversible YCoCg, R / B overflow folding, round half up, nearest chroma.
- [ ] Hap Q colour: integer scaled-YCoCg inverse with D = B + 8 and offset 255/2, round half up.
- [ ] Padding never shows: no stray column or row at the right or bottom edge at any size or scale.
- [ ] Chroma planes are sized ceil(w/2) × ceil(h/2); a 1-pixel-tall HQ clip does not crash.
- [ ] No sRGB / gamma conversion is applied to decoded values.

### Alpha
- [ ] DXV clips with alpha show `Alpha Type: Premultiplied` by default.
- [ ] Choosing `Straight` multiplies colour by alpha before compositing; choosing `Premultiplied` draws texels as stored.
- [ ] A straight file shown as Straight and a premultiplied file shown as Premultiplied both match the reference composite over black and over the checkerboard (≤ 1).
- [ ] DXT1 and YCG6 are unaffected by the setting.
- [ ] The control is in the transport bar, in View ▸ Alpha Type, and on the bare `A` key; all three stay in sync.
- [ ] The choice is remembered per clip by content key (SHA-256 of `"<bytes> <frames> <w>x<h>"` + first packet + last packet) and applied before the first frame is drawn.
- [ ] A renamed or moved copy keeps its choice; different content at the same path starts at Premultiplied.
- [ ] At most 2,000 remembered clips; the oldest is dropped first.
- [ ] The hint reads `looks premultiplied`, `looks straight` or `can't tell` using thresholds 500 samples, ≤ 0.5 %, ≥ 3 % on the first and middle frames; it never changes the setting.
- [ ] Hap5, HapM, HapA show `Alpha Type: Straight (HAP)` greyed, and are composited as straight.
- [ ] ProRes 4444 and HEVC-with-alpha show the stated type and codec name, greyed.
- [ ] Opaque clips show no Alpha Type control and the menu items are disabled.

### Background
- [ ] View ▸ Background ▸ Black / Checkerboard; default Black; the choice survives a relaunch.
- [ ] Checkerboard: 8-point cells at any display scale, greys 0.85 and 0.65, only under the picture, never in the letterbox.
- [ ] Opaque clips look identical in both modes.
- [ ] Switching the background or the Alpha Type redraws the current frame at once, also while paused.

### Transport
- [ ] Every clip opens playing and looping.
- [ ] Space toggles play / pause exactly once per press.
- [ ] `L` toggles loop; the title shows `[LOOP]` while looping; the bar's loop button is lit.
- [ ] With loop off, playback stops on the last frame (AV path: next Play restarts from frame 0).
- [ ] `,` / `.` step exactly one frame back / forward, pause playback, keep audio silent and aligned, and clamp at both ends.
- [ ] ← / → seek 5 s; with Shift 0.5 s; bar buttons seek 10 s.
- [ ] Restart seeks to frame 0 and keeps the play / pause state.
- [ ] Scrub slider: dragging pauses, shows each scrubbed frame live, and resumes on release only if it was playing.
- [ ] Scrubbing never builds a backlog: the frame shown is the latest requested, within about one decode time.
- [ ] A 1-frame clip shows its frame.
- [ ] On the system-codec path, seeking to frame N shows frame N at 23.976, 25, 29.97, 30, 60 fps and on jittered timing (the 8 `frames_*` fixtures, 40 of 40 each).
- [ ] On the system-codec path the loop wraps onto frame 0 exactly.
- [ ] Time label `M:SS.mmm`, frame number label, both updating at about 20 Hz, also while a slider is held.
- [ ] Esc quits only when: not full screen, no dialog, a file is open, no "not supported" message, and playing. Otherwise Esc keeps its normal meaning.
- [ ] `F` and the bar button toggle full screen; the OS's own full-screen shortcut also works.

### Audio
- [ ] DXV and HAP clips with an audio track play it in sync; the video frame follows the audio position (round to nearest frame).
- [ ] Clips without audio play on the wall clock with no drift across loops.
- [ ] If audio fails to start within 0.25 s, video keeps playing on the wall clock.
- [ ] Audio loops with the video; seek and frame step move audio to the same position.
- [ ] `M` and the speaker button mute without stopping the clock; unmute restores the previous level.
- [ ] Volume: slider 0…1, `-` / `=` and ↑ / ↓ and ⌘↑ / ⌘↓ step 0.1, clamped; the level carries over to the next clip in the session.
- [ ] Speaker icon changes at muted / zero, < 0.33, < 0.66, else.
- [ ] Malformed audio formats (0 or > 64 channels, non-finite or ≤ 0 sample rate) are rejected and video still plays.

### Folder navigation and media keys
- [ ] `[` / `]`, ⌘← / ⌘→ and the bar buttons step through the movie files of the current clip's folder in natural (Finder) order, wrapping at both ends.
- [ ] Hidden files are skipped; the buttons are dimmed and disabled when there is no sibling.
- [ ] OS media keys: play / pause, previous, next act on Glance; the OS "now playing" surface shows file name, duration, position and can seek.

### Window and readouts
- [ ] Default content size 960 × 616; View ▸ Default Window Size (⌘0) restores and centres it.
- [ ] Minimum content size 500 × 180.
- [ ] On each clip open the width snaps to the clip's aspect at the current height, clamped to [500, 95 % of the screen width]; the height never changes.
- [ ] Video is aspect-fit with black bars; no stretch.
- [ ] Title `<file name>  [<codec label>]  [LOOP]`; codec labels exactly as in section 2.1.
- [ ] Info line `W×H · NNfps · M:SS` (fps and duration formats as in 2.1) in the title area and at the right of the bar; the bar copy truncates first.
- [ ] Transport bar is 40 pt high, scrub row 36 pt, dark grey; all controls have the tooltips listed in 2.6.

### Settings
- [ ] Exactly three persisted values: background mode, per-clip Alpha Type list, default-prompt-shown flag.
- [ ] Volume, mute, loop and last file are not persisted.

### Release
- [ ] Build refuses to proceed when the two version fields differ.
- [ ] Release artifact is signed, notarised and stapled; a checksum file is published beside it.
- [ ] Release is a GitHub Release on `AmritusG/glance-releases`, tag `v<version>`, title `Glance <version>`, body = that version's notes file.
- [ ] After release exactly one registered copy of the app remains on the build machine.
- [ ] No auto-updater.


## CueGlance

Detail and sources: `docs/research/cueglance.md`.

### App shell
- [ ] Single window titled "CueGlance", min content 880 × 720, default 1100 × 880; closing it quits
- [ ] Title shows `<file name>  [<codec>]  [LOOP]` (two-space separators), or "CueGlance" with no clip
- [ ] Subtitle shows `W×H · NNfps · M:SS` with the fps and duration label rules of 2.1
- [ ] Dark theme with the palette values of `Theme.swift:37-66`
- [ ] Layout: metadata bar, scrolling middle (video pane, handle, envelope, handle, cue list), pinned footer
- [ ] Video pane height 280 default, drag-resizable 160–720; envelope 200 default, 130–540; not persisted
- [ ] App starts with no clip, an "Untitled" project with Start (0) and End (60), preset "Envelope"

### Menus and keys
- [ ] File: New ⌘N, Open… ⌘O, Open Recent (max 5, "(No Recent Projects)", Clear Menu), Save ⌘S, Save As… ⇧⌘S, Load Video… ⌘L, Export Resolume XML… ⌘E (disabled when not exportable)
- [ ] Edit: Undo ⌘Z, Redo ⇧⌘Z; also Ctrl+Z / Ctrl+Y anywhere
- [ ] View: Background (Black / Checkerboard, checkmark follows state), Alpha Type (Premultiplied / Straight), Toggle Alpha Type; the alpha items disabled unless the clip is choosable
- [ ] Quit ⌘Q and window close both run the unsaved-edits prompt; Cancel aborts
- [ ] ⌘. toggles play / pause
- [ ] Video focused: Space, ←/→ (5 s, Shift 0.5 s), ↑/↓ (volume 0.05, Shift 0.01), `,` / `.`, `m`, `l`/`L`, `f`/`F`
- [ ] Outside text fields: `c` adds a cue at the live time; `a` toggles alpha when choosable; 1–9 recall the Nth cue; Return recalls the selected cue; `[` / `]` step cues; ⌥ with a recall key also starts playback
- [ ] None of the bare keys fire while a text field is being edited

### Opening files
- [ ] Load Video panel (single selection, movie / video types) and the empty-state "Load Video…" button
- [ ] Drop a file on the video pane; with several files the first is opened
- [ ] Open… accepts `.cueproj` and `.json`, opens the single sibling video with the same base name; 0 or >1 siblings show the two alerts and open nothing
- [ ] Opening any clip with unsaved cue edits prompts Save / Discard / Cancel first; Cancel keeps the current clip
- [ ] Opening a clip loads `clip.cueproj` when present and valid, else a fresh Start/End envelope; selection and undo history are cleared; the document is clean
- [ ] Opening a clip whose sidecar `trackDuration` equals the clip length (to the ms) leaves the document clean, on every engine, also when switching from another clip
- [ ] A clip of the same length as the previous one still re-syncs `trackDuration`
- [ ] A missing or invalid sidecar never raises an error

### Routing and playback
- [ ] Route by content: DXV (`DXD3`, `DXD4`, `DXDI`; variant from the first packet) → DXV engine; `Hap1`/`Hap5`/`HapY`/`HapM`/`HapA` → HAP engine; else the platform decoder
- [ ] H.264, HEVC (incl. alpha), ProRes (incl. 4444 alpha), Motion JPEG play on both platforms
- [ ] MKV, WebM/VP9, QuickTime Animation show `"<codec> in <CONTAINER> is not supported"` with the exact names of 3.2 (decide whether Rust keeps this limit or plays more)
- [ ] A DXV/HAP file that cannot be opened or decoded shows the four exact messages of 2.12; no crash, no stale frame
- [ ] An unplayable clip still loads, edits and saves its cues, and its saved `trackDuration` is not reset
- [ ] Every clip opens playing, looping, from frame 0; a 1-frame clip shows its frame
- [ ] Transport: prev cue, play/pause, next cue, restart, ±10 s, frame step ±1 (pauses), loop toggle, mute, volume slider, fullscreen; all disabled with no clip
- [ ] Non-looping clip holds the last frame; Play then restarts from 0
- [ ] Scrub bar: click and drag seek; playback pauses during the drag and resumes after; latest target wins
- [ ] Timeline, envelope playhead and readout follow the pointer during a scrub
- [ ] Time readout `MM:SS.mmm / MM:SS.mmm`, rounded (frame 60 at 25 fps reads 00:02.400)
- [ ] Audio plays on every path; mute and volume (linear 0–1) apply to whichever engine is active and survive a clip switch
- [ ] DXV/HAP with audio: video follows the audio position (`round(audioSeconds × fps)`); falls back to the wall clock if audio does not start within 0.25 s
- [ ] Stale async results from a previous open never update the UI

### Frame exactness (non-DXV/HAP path)
- [ ] Seek to N / fps shows and reports frame N for all 40 frames of each of the 8 `frames_*` fixtures (0 wrong)
- [ ] Frame step both ways across the GOP boundary at 12 shows the right frame; clamps at 0 and 39
- [ ] Rate reported as the exact rational (24000/1001, 25/1, 30000/1001, 30/1, 60/1) by the rule order of 4.4
- [ ] Frame starts come from the sample table through the edit list, not from a grid
- [ ] DXV/HAP seeks are by frame index (`round(t × fps)`), clamped

### Rendering
- [ ] DXV DXT1/DXT5/YCG6/YG10 and legacy DXV render equal to the reference CPU decode at 1:1 (max difference 0)
- [ ] HAP Hap1/Hap5/HapY/HapM/HapA render equal to the CPU HAP decode at 1:1
- [ ] Widths and heights that are not multiples of 16 or 4 show no shear and no padding at any scale
- [ ] BC blocks decoded in the shader from integer textures with the integer palettes of 4.3
- [ ] YCG6/YG10 use the Arena YCoCg maths (midpoint 127.5, nearest chroma, R/B folding, round at the end)
- [ ] Hap Q / HapM use the integer scaled-YCoCg inverse
- [ ] Picture is aspect-fit in the pane; letterbox black
- [ ] Background Black: transparent pixels read black; Checkerboard: 8 pt cells, greys 0.85 / 0.65, DPI-scaled, only under the picture; same look on every engine
- [ ] Background choice applies at once (also to a paused frame) and persists

### Alpha Type
- [ ] DXV alpha clips (DXT5, YG10, legacy DXT5) show the switch, default Premultiplied, with the hint text; thresholds as in 4.1
- [ ] Straight premultiplies in the shader; Premultiplied draws as stored; a paused frame redraws on change
- [ ] Hap5 / HapM / HapA always treated as straight; shown greyed "Alpha Type: Straight (HAP)"
- [ ] ProRes 4444 / HEVC alpha shown greyed with the stated type and codec name
- [ ] Opaque clips show no alpha control
- [ ] The type is applied before the first frame is shown
- [ ] With a sidecar on disk: the choice is a project edit (dirty, one undo step, saved with ⌘S); undo/redo changes the picture
- [ ] Without a sidecar: remembered at once by content key, document stays clean; a moved or renamed copy keeps it; different content at the same path starts at the default
- [ ] A remembered choice is written into the sidecar on its first save
- [ ] The content key equals the Swift app's SHA-256 hex for the same file (so existing memories carry over, if the Rust app shares them)

### Cue editing
- [ ] Add cue at the live playback position (audio master when available), named "Cue N", colour cycling the 11 non-green presets, selected, undoable
- [ ] First and last cue by time cannot be removed and cannot move in X; their Y is editable
- [ ] Remove deletes the selected non-boundary cue
- [ ] Table columns and field rules of 2.6 (steps 0.001 and 0.5, ranges, formats, disabled states)
- [ ] Numeric fields commit on Enter and focus loss, accept a comma or point decimal, clamp, revert bad input
- [ ] Row click selects and seeks; the scope button recalls; timeline marker click selects and seeks
- [ ] Colour popover with the 12 presets; writes `rgb(r, g, b)`
- [ ] Curve picker lists the 23 types by category; unknown ids shown as "Curve N" and preserved
- [ ] Disabled cues are dimmed, excluded from the curve and the export, still saved
- [ ] Duration field edits move the End cue; a clip's real length overrides `trackDuration` when it lands
- [ ] Undo/redo restore whole-project state; redo works after undo; history cleared on clip open and New

### Envelope canvas
- [ ] Curve through enabled cues with the segment A→B using B's curve; preview maths equal to `CurveType.evaluate`
- [ ] Drag a point: Y 0–100, X between its neighbours (±1 ms), boundary X locked; Lock X / Lock Y; one undo step per drag
- [ ] Grid, labels, Y chips, playhead as in 2.7

### Files
- [ ] `.cueproj` read: the 6 required top-level keys + optional `alphaType`; 7 cue keys; unknown curve ids and colours tolerated
- [ ] `.cueproj` written byte-identical to the Swift app for the same project (sorted keys, 2-space indent, `" : "`, `\/`, number shapes, no trailing newline, atomic write); `alphaType` omitted when unset
- [ ] A sidecar saved by Swift CueGlance, loaded and saved untouched, is byte-identical
- [ ] Decide and document what happens to unknown top-level keys (Swift drops them)
- [ ] After load, missing Start / End cues are appended
- [ ] Save writes `clip.cueproj` next to the clip; with no clip it falls back to the last project path, else Save As
- [ ] Nothing is written to disk without an explicit Save
- [ ] New: prompt, unload the clip, fresh envelope, nothing on disk changes, no frame left on screen
- [ ] Export XML byte-identical to `CueSync.xml` for `CueSync.cueproj` with the same `uniqueId`
- [ ] Export rules: enabled only, normalise, synthesise x=0 / x=1, stable sort, tabs, JS-style numbers, escaping, `altName` fallback, epoch-ms `uniqueId`
- [ ] Export refuses with an alert on zero duration or no enabled cue; default file name is the sanitised preset name
- [ ] Export panel starts in the default export folder when it exists

### Settings
- [ ] Persist background mode, alpha memory (cap 2,000, newest last), default export folder, recent projects (cap 5)
- [ ] Default export folder is the Resolume Arena Envelopes folder under Documents (define the Windows path)
- [ ] Footer pill shows the folder (home collapsed), click to change, icon to reveal it (or the nearest existing parent)
- [ ] Volume, mute, loop, pane sizes and locks reset each launch

### Release
- [ ] Signed, notarised macOS build and DMG `CueGlance-v<version>.dmg` + `.sha256`
- [ ] Windows installer and signing (new; nothing to match)
- [ ] Version shown consistently in the bundle, tag, file name and notes
- [ ] Exactly one LaunchServices record after a release on macOS

---


## GlEnc

Detail and sources: `docs/research/glenc.md`.

### Identity and file handling
- [ ] The app never registers itself as the default handler for any movie type; a double-click on a `.mov` / `.mp4` never opens GlEnc.
- [ ] Files can still be sent to the app: drop on the window, drop on the app icon, "Open with", command-line open with several paths (all paths become jobs).
- [ ] Paths that arrive before the window exists (cold launch) are queued once the UI is ready.
- [ ] A drop of non-movie files adds no rows and shows no error.
- [ ] "Add files…" picker is multi-select and limited to movie types.
- [ ] One main window; its size and position survive a relaunch.

### Input decode
- [ ] DXV3 DXT1 / DXT5 / YCG6 / YG10 sources decode (sample-entry tags `DXD3`, `DXT1`, `DXT5`, `YCG6`, `YG10`).
- [ ] `DXD4` sources decode.
- [ ] Legacy DXV (`DXDI`) sources decode.
- [ ] A file whose sample-entry tag is the literal `DXV3` is rejected with a clear error.
- [ ] Hap1 / Hap5 / HapY / HapM sources decode, single-section and chunked.
- [ ] A HapA-only file is rejected as a source.
- [ ] ProRes 422 family, ProRes 4444 (with alpha), H.264, HEVC, HEVC with alpha, Motion JPEG sources decode.
- [ ] A source with width or height < 1 or > 65535, or more than 16384² pixels, or a non-finite / zero frame rate fails at add time with "Unusable source: …".
- [ ] A file without a video track fails with "source has no video track".
- [ ] Only the first video track and the first audio track are used.

### Output: DXV
- [ ] DXT1, DXT5, YCG6, YG10 files open in Resolume Arena and decode with DXVKit and ffmpeg.
- [ ] Sample entry is `DXD3`, 86 bytes, vendor `FFMP`, temporal / spatial quality 512, 72 dpi, frame count 1, empty compressor name, depth 24, colour table id `0xFFFF`, no extension atoms.
- [ ] `tkhd` and the sample entry carry the presentation size; odd and non-16 sizes round-trip at the exact size.
- [ ] `ftyp` is `qt  ` / `0x200` / `qt  `; `wide` precedes `mdat`; `moov` is at the end.
- [ ] One sample per chunk (`stsc` 1 / 1 / 1; one `stco` entry per frame).
- [ ] A file whose `mdat` passes 4 GB uses the largesize `mdat` header and `co64`; a file under 4 GB uses `stco`.
- [ ] `udta/©swr` reads `GlEnc <version>`.
- [ ] DXV encode uses the encoder's best effort setting; there is no quality control in the UI.

### Output: HAP
- [ ] Hap1 frame = section `0xBB` with Snappy BC1; Hap5 = `0xBE` with Snappy BC3; HapY = `0xBF`; HapA = `0xB1`.
- [ ] HapM frame = outer `0x0D` containing the HapY section first, then the HapA section.
- [ ] Section header is 4 bytes below 16 MiB of payload and 8 bytes (three zero bytes, type, 32-bit LE length) at or above.
- [ ] Chunks = 1 gives byte-identical output to the single-section form.
- [ ] Chunks 2…64 give `0xCB` / `0xCE` / `0xCF` / `0xC1` with a `0x01` container holding a `0x02` compressor table and a `0x03` size table (stored sizes), no `0x04` table; HapM keeps `0x0D` outside and chunks both inner sections.
- [ ] A chunk that Snappy does not shrink is stored raw (`0x0A`).
- [ ] Decoded pixels are identical for every chunk count.
- [ ] Coded size is the presentation size rounded up to a multiple of 4; the file's stated size is the presentation size.
- [ ] HapY uses Y = (R + 2G + B + 2) >> 2, Co = (R − B) >> 1, Cg = (−R + 2G − B) >> 2 with per-block scale 4 / 2 / 1 stored as blue 24 / 8 / 0.
- [ ] HapA of a source without alpha fails with "HapA requires a source with an alpha channel".
- [ ] HapM of an opaque source succeeds with alpha 255 everywhere.
- [ ] Hap1 / Hap5 / HapY / HapM files open in Resolume Arena.

### Output: ProRes / H.264 / HEVC / MJPEG
- [ ] ProRes 422, 422 HQ, 422 LT, 422 Proxy, 4444 write `.mov` with tags `apcn`, `apch`, `apcs`, `apco`, `ap4h`.
- [ ] ProRes 4444 keeps the source alpha; the 422 family shows "alpha will be flattened" for alpha sources.
- [ ] Choosing ProRes picks 4444 for an alpha source and 422 otherwise.
- [ ] H.264 and HEVC write `.mp4` (default) or `.mov`.
- [ ] H.264 / HEVC: constant quality 0.1–1.0 (default 0.6) or average bitrate in Mbps; keyframe interval in frames (0 = auto); H.264 profile Baseline / Main / High (default High).
- [ ] Motion JPEG writes `.mov` at quality 0.85 with no options.
- [ ] H.264 / HEVC crop and custom sizes are forced to even numbers.
- [ ] These outputs keep the source frame timing (variable frame rate is preserved).

### Alpha
- [ ] DXT5, YG10, Hap5, HapM outputs store straight colour and straight alpha for straight sources (decoded colour = source colour where alpha > 0, within block error).
- [ ] A DXV source with alpha and Alpha Type = Premultiplied yields straight colour in every output.
- [ ] A DXV source with Alpha Type = Straight is passed through unchanged.
- [ ] Un-premultiply uses `min(255, (c × 255 + ⌊a/2⌋) / a)`, `a = 0 → 0`; pixels with alpha 255 are untouched.
- [ ] Alpha Type default is Premultiplied; it is editable per job, in batch, in the Queue menu, as a default for new files and on the command line.
- [ ] The hint reads "looks premultiplied" (≤ 0.5 %), "looks straight" (≥ 3 %) or "can't tell" (in between, or under 500 samples) and never changes the setting.
- [ ] HAP alpha, ProRes 4444 and HEVC-alpha sources show the Alpha Type control disabled with the stated value.
- [ ] An HEVC-with-alpha source that states premultiplied alpha is un-premultiplied on read when the output keeps alpha.
- [ ] Opaque outputs (DXT1, YCG6, Hap1, HapY) take the colour as read with alpha forced to 255.
- [ ] Output files written to byte-match v1.6.2 for the same source and settings (DXV and HAP paths).

### Frame rate
- [ ] 23.976, 24, 25, 29.97, 30, 48, 50, 59.94, 60, 100, 120 fps sources are read at exactly those rates.
- [ ] A 40-frame 60 fps clip reads 60 (not 59.94); a 25 fps clip with jittered sample durations reads 25.
- [ ] DXV / HAP output timescale is `fps × 512` with delta 512 for integer rates, and 24000 / 30000 / 60000 with delta 1001 for the NTSC rates.
- [ ] A DXV / HAP job at a rate outside that set fails with a clear message.

### Trim, crop, resize
- [ ] In / Out are inclusive frame indices; the output contains exactly frames In…Out.
- [ ] Progress of a trimmed job runs 0–100 % over the trimmed range.
- [ ] Decision recorded on audio for trimmed jobs (v1.6.2 muxes the full-length source audio).
- [ ] Crop is in source pixels, top-left origin, applied before resize.
- [ ] Crop overlay has four corner handles, interior drag, dim mask, optional clip-boundary outline.
- [ ] Arrow keys nudge the crop by 4 px (16 px with Shift); Esc cancels the edit; frame-step arrows are inactive during a crop edit.
- [ ] Numeric crop fields W / H / X / Y clamp to the source and round to the codec alignment.
- [ ] Size menu: Original (W×H), 12 fixed presets, saved sizes, Custom… (1–16384 per side), Manage Sizes….
- [ ] "Original" after a crop encodes at the crop size and shows that size.
- [ ] Resize quality Auto = Lanczos for a downscale, Bilinear otherwise; Nearest keeps hard edges.
- [ ] Fit (letterbox) centres the picture with opaque black bars; Distort stretches; equal aspect gives no bars.
- [ ] No crop and no resize passes source frames through untouched.

### Audio
- [ ] Audio On carries the first source track into the output; Off strips it.
- [ ] `.mov` outputs carry 32-bit signed little-endian PCM; `.mp4` outputs carry AAC at 256 kbit/s.
- [ ] Rate menu Original / 44.1 / 48 / 88.2 / 96 kHz resamples.
- [ ] `.mp4` caps audio at 48 kHz (higher choices disabled; a higher source rate is resampled down).
- [ ] 88.2 / 96 kHz PCM in DXV / HAP files plays at the right speed (`lpcm` version-2 entry).
- [ ] Mono, stereo and multichannel sources keep their channel count.
- [ ] A source without audio shows "No audio track" and produces no audio track.
- [ ] An audio failure never fails the job: the video is written and the row shows "Done · no audio" with the reason.
- [ ] The video bytes of a DXV / HAP file are identical with and without audio.

### Queue, naming, collisions
- [ ] Jobs encode one at a time in list order; statuses Queued / Encoding / Done / Failed.
- [ ] Per-row Cancel, Retry, Show (reveal in the file manager), Remove.
- [ ] Cancel stops within one frame and deletes the partial file; Cancel All also fails the remaining queued rows as "Cancelled".
- [ ] A failed encode leaves no file behind.
- [ ] Auto names follow `<stem>_DXV Normal Quality[ With Alpha]`, `<stem>_DXV High Quality[ With Alpha]`, `<stem>_Hap1` / `_Hap5` / `_HapY` / `_HapM`, `<stem>_ProRes 422[ HQ| LT| Proxy]`, `<stem>_ProRes 4444`, `<stem>_H.264`, `<stem>_HEVC`, `<stem>_MotionJPEG`.
- [ ] A crop adds ` [WxH]`; a trim adds ` [MM.SS.CC-MM.SS.CC]` or ` [N-M]` per the preference.
- [ ] The output name is editable and has a reset-to-auto button; an edited name is not overwritten by later setting changes.
- [ ] Output goes next to the source, or to a fixed folder when one is set and exists.
- [ ] Collision policy Ask / Overwrite / Auto-rename; Ask offers Overwrite / Rename / Skip / Cancel and "apply to all" for the session.
- [ ] Three same-named jobs in one batch under Auto-rename produce `name`, `name_2`, `name_3`.
- [ ] A re-run never prompts and writes the first free `name_N` (N ≥ 2), not colliding with other queued jobs.
- [ ] Multi-select; with 2+ rows selected the top bar edits those rows, otherwise the defaults for new files; mixed values show "Mixed".
- [ ] Rows that are encoding or done are not changed by a batch edit.
- [ ] Reset Selected, Reset All Finished, Select Finished Clips, Clear Completed, row context menu "Reset for Re-run".
- [ ] Header shows the job count and overall progress.

### Preview
- [ ] Selecting a row loads it; Source / Output toggle (Output enabled when the job is done and the file exists).
- [ ] Play / pause, frame step, loop, scrub bar with In / Out markers, timecode `HH:MM:SS:FF`, typed In / Out timecodes.
- [ ] Playback loops inside the trim window when loop is on, and stops at Out when it is off.
- [ ] DXV, HAP and other sources all preview, including HAP with audio.
- [ ] Sources with real alpha show a checkerboard through transparent pixels; opaque sources never do; an opaque ProRes 4444 file is treated as opaque.
- [ ] Straight-alpha frames are composited correctly (no darkening, no vanished video).
- [ ] Checkerboard extent preference: fill viewport / behind video only.
- [ ] Preview decode never blocks the UI thread; under load the newest frame wins.

### Settings and shortcuts
- [ ] All 20 settings in section 7.2 persist with the same defaults.
- [ ] "Reset to Defaults" restores the factory values.
- [ ] Shortcuts: preferences, show / hide preview (⇧⌘P), select all clips (⇧⌘A), reset selected (⌘R), reset all finished (⇧⌘R), select finished (⌥⌘A), encode queue (⌘↩), cancel all (⌘.), Space, ← / →, I, O.
- [ ] Queue menu "Change Selected Clips" / "Change All Clips" with the ten submenus.

### CLI
- [ ] `glenc-cli <in> <out> --codec <c>` for all 17 codec names; prints the output path on success; non-zero exit and no file on failure.
- [ ] `--container`, `--width` / `--height`, `--resize-quality`, `--aspect`, `--crop x:y:w:h`, `--trim start:end`, `--no-audio`, `--chunks 1…64`, `--alpha-type`, `--writer-version`, `--verbose` behave as in section 2.18.
- [ ] CLI and GUI produce byte-identical DXV / HAP files for the same settings.

### Release
- [ ] Signed, notarised installer per platform with a published SHA-256.
- [ ] Version string in the app, the installer name and the `©swr` tag agree.
- [ ] Public source mirror and release page updated per release.

---


## MaCrate

Detail and sources: `docs/research/macrate.md`.

Flat, testable items. Section references point at the facts above; `db` = `macrate-db.md`. Numbers are the Swift app's and are the pass criteria unless the owner changes them.

### Library roots and folders
- [ ] Add one or several folders through a folder picker; add by dropping folders (a dropped file adds its parent) on the window (§2.1)
- [ ] Adding an exact duplicate root is refused; a parent/child overlap is added with a warning; the result message counts added / overlapping / already present (§2.1)
- [ ] Roots refuse to index `/`, `/Users`, `/Volumes` exactly and anything at or under `/System`, `/Library`, `/Applications`, `/private`, `/usr`, `/bin`, `/sbin`, `/etc`, `/var`, `/tmp` (§2.1) — with an equivalent list on Windows
- [ ] Remove a root from the sidebar; its database rows stay; the view is cleared if it showed that root (§2.1)
- [ ] Root sort modes Manual / Name ↑↓ / Date Added ↑↓ / Size ↑↓ persist; manual reorder persists; "Show Sizes" persists and shows the sum of indexed clip sizes (§2.1)
- [ ] A root on an unmounted volume shows as unavailable with Reconnect and Remove; mounting the volume makes it available again without a re-index; unmounting clears a selection inside it (§2.1)
- [ ] Per-folder "Include subfolders" toggle affects browsing only, default on, persists (§2.1)
- [ ] Exclude / Include a subfolder: excluding removes its rows from the index and from every query, sidecars stay on disk; including re-indexes; an excluded folder shows the explanatory empty state (§2.1)
- [ ] Sidebar folder tree lists directories lazily, sorted case-insensitively; expanded folders and the selected folder are restored on launch (§2.5, §2.20)

### Scanning, watching, indexing
- [ ] Only files with extension `mov mp4 m4v avi mkv mxf webm` (case-insensitive) are clips; hidden files, package contents and symlinks are skipped (§2.2)
- [ ] `clip_id` of a file equals the Swift value for the vectors in db §i.1 item 5
- [ ] A file whose size and mtime (±0.001 s) match its row is not re-hashed (§2.2)
- [ ] New file in a watched folder appears without restart within about 1 s of the write settling; deleted file becomes an orphan row, not a deleted row (§2.2)
- [ ] File renamed or moved outside the app keeps its metadata: new path gets a row with the sidecar's fields, the old row is orphan-flagged and then retired once the content is live elsewhere (§2.2, db §g.4)
- [ ] A folder moved into a root is scanned recursively on the first event (§2.2)
- [ ] The app's own sidecar writes do not trigger a grid reload; a sidecar changed by another process on a mounted library updates the row and the open editor (§2.2, §2.7, db §g.4)
- [ ] Events on dot-files and atomic-write temporaries are ignored (§2.2)
- [ ] Duration, fps and pixel size are read from the container header without a media framework; fps `0` / size `0×0` mean "tried, none" and are not retried until the file changes (§2.3, db §d.2)
- [ ] Rotated clips report the displayed (rotated) size; DXV / HAP report display size, not padded coded size (§2.3)
- [ ] Indexing progress shows root name and "N of M"; a no-op event in a watched folder does not flash the indicator (§2.5)

### Thumbnails
- [ ] One poster per clip, chosen as the first frame within the first 15 frames / 1.0 s whose 32×32 mean luminance > 0.10 or max > 0.30, else the brightest seen (§2.4)
- [ ] DXV (DXT1, DXT5, YCG6, YG10, legacy) and HAP (Hap1, Hap5, HapY, HapM) thumbnails decode with the project's own decoders; other codecs through the platform decoder (§2.3, §2.4)
- [ ] Alpha clips show a baked checkerboard (about 10 tiles on the long edge); DXV alpha honours the clip's Alpha Type and the thumbnail updates when it changes (§2.4, §2.7)
- [ ] Stored thumbnails are JPEG, long edge ≤ 1280 px; at most 4 decodes run at once; memory cache bounded (1 GiB / 500 images in Swift) (§2.4)
- [ ] A changed file (size or mtime) gets a new thumbnail; deleting the cache directory is harmless (§2.4, db §h.3)
- [ ] 1-pixel-tall and odd-sized HQ DXV clips thumbnail without crashing (`crate/CLAUDE.md:159`)

### Grid and browsing
- [ ] Adaptive grid with cell sizes 100 / 140 / 180 / 240 / 320, default 180, spacing 12; cells keep clip aspect (§2.5)
- [ ] Cell shows filename and, per preference, length · fps · resolution, rating, tags, BPM, mood, genre, energy, brief; defaults on: length, rating, fps, resolution (§2.5)
- [ ] Cell alignment Top / Center / Bottom and the four metadata layouts (below, scrim, dim text, reveal on hover) persist (§2.5)
- [ ] Count text shows "N files" / "N matches"; empty states for no folder, empty folder, no matches, searching, indexing (§2.5)
- [ ] A background re-enumeration that yields the same file list does not move the scroll position or flash a spinner (§2.5, §2.19)
- [ ] Editing one clip refreshes that cell only (§2.19)
- [ ] Info panel shows name, size, modified date, duration, fps, resolution, Alpha Type control, full path, duplicates disclosure, Measured box, metadata editor (§2.5)

### Selection, keyboard, mouse
- [ ] Click, ⌘/Ctrl-click toggle, Shift-click range, select-all, rubber-band marquee (Shift = additive), click on empty space clears (§2.8)
- [ ] Arrow keys: left/right previous/next; up/down one row in the grid, previous/next while previewing; clamped at the ends; grid scrolls to the selection (§2.17)
- [ ] Space opens the preview of the selected clip; inside the preview Space closes (default) or plays/pauses (preference); Esc closes; P play/pause; `,` `.` step one frame; L loop; B flip checkerboard/black (§2.17)
- [ ] None of these keys fire while a text field has focus (§2.17)
- [ ] Menu commands and shortcuts: Settings popover (⌘J), Settings window (⌘,), Import Briefs… (⇧⌘I), Undo Rename (⌘Z), View ▸ Alpha Type, About, Acknowledgements… (§2.17)

### Search, filter, sort
- [ ] Typing in search switches to whole-library results after 150 ms; clearing search and filters returns to the folder (§2.6)
- [ ] Search matches filename (without extension), tags, mood, genre and brief by word prefix, all terms required; punctuation and `_` `-` split words; an all-punctuation query returns nothing (db §f.2, §f.5)
- [ ] Filters: rating ≥ N, tags with match all / any, BPM range, energy range, mood, genre, has duplicates; each shows a removable chip; "Clear all" also resets the sort (§2.6)
- [ ] Measured filters with "include not measured": measured by, loops only, cycle range, exact cycles, join step ≤, no hard cuts, no flashes, moves / still, no text, on black / full frame, centre-heavy (§2.6)
- [ ] 16 sort keys with the Swift labels; nil values sort last in both directions; Random reshuffles only when re-picked (§2.6)
- [ ] In folder view, rating / BPM / energy / loop / cycle sorts fall back to filename A→Z (current behaviour — confirm with the owner whether to keep) (§2.6)
- [ ] Results never include orphans or clips on unmounted volumes; mounting a volume refreshes results (§2.6, db §f.5)
- [ ] Presets: save, recall, rename, delete; names unique; a preset stores filter and explicit sort; presets written by the Swift app load in the Rust app and the reverse (§2.6, db §e.7)
- [ ] Search text, filter and explicit sort are restored on launch (§2.20)

### Metadata editing
- [ ] Rating 0-5 (click current star clears), tags (lowercased, trimmed, unique, sorted), BPM, mood, genre, brief, energy 1-10 or unset (§2.7)
- [ ] Rating / tags / energy save immediately, text fields after 0.3 s; pending edits flush on selection change and before rename; save badge Saving… / Saved / Save failed + Retry (§2.7)
- [ ] Every save writes the sidecar first and only then the database; a failed sidecar write is reported and nothing is marked saved (§2.7, db §h.1)
- [ ] Tag, mood and genre autocomplete from existing values (§2.7)
- [ ] Rename a clip: validation, collision refusal, sidecar moves with the file, database row and tags follow, single-level undo with its three refusal cases (§2.7)
- [ ] Alpha Type menu for DXV alpha clips, default Premultiplied, remembered per content (`clip_id`), shared by duplicates, survives move/rename; HAP / ProRes 4444 / HEVC alpha show their stated value read-only (§2.7, db §d.2)
- [ ] No operation deletes or moves a media file except Rename (§2.7)

### Orphans and duplicates
- [ ] Orphan list with Re-attach (hash must match), Forget (deletes sidecar and row after confirmation), Dismiss all; dismissed list with Restore / Re-attach / Forget; sidebar counts (§2.5, §2.2)
- [ ] A dismissed orphan whose file reappears becomes live again automatically (db §f.1)
- [ ] Duplicate badge, info-panel list of other paths (click navigates), "has duplicates" filter, Library Duplicates sheet, Refresh Duplicate Markers (§2.2)
- [ ] Tags are per file; briefs fan out to every copy of the same content (db §f.1)

### Stashes
- [ ] Stashes found in `.crate-stashes.json` at each root appear in the sidebar with clip counts after launch, add-root and Refresh (§2.8, db §h.2)
- [ ] Opening a stash shows its clips in stash order; entries resolve by relative path first, then by `clip_id`; unresolved entries are hidden but kept (§2.8)
- [ ] Rename, delete and drag-reorder write the JSON back and are disabled when the owning drive is not mounted (§2.8)
- [ ] The selected stash is not restored on launch today (`crate/CHECKPOINT.md:43`) — decide whether to keep

### Preview playback
- [ ] Preview opens over the grid, autoplays from frame 0, loops by default; closing returns to the same grid position (§2.9)
- [ ] Player chosen by sample-entry FourCC: `DXV3 DXD3 DXD4 DXDI` → DXV; `Hap1 Hap5 HapY HapM` → HAP (standalone `HapA` has no driver today and previews blank); everything else → standard decoder (§2.9)
- [ ] DXV and HAP render pixel-exact against the reference decoders, including non-16-aligned sizes and portrait clips (§2.9, `crate/CLAUDE.md:150-161`)
- [ ] Release-build HQ DXV decode stays inside the frame budget (Swift reference: YCG6 ≈ 10 ms, YG10 ≈ 16 ms per frame against a 33 ms budget at 30 fps) (`crate/CLAUDE.md:175-189`)
- [ ] Standard-codec stepping lands on exact frames for 23.976 / 25 / 29.97 / 30 / 60 fps and jittered timestamps (fixtures `Tests/CrateTests/Fixtures/frame-timing`) (§8.2)
- [ ] Transport: play/pause, scrub with live seek, time readout M:SS.mmm, loop toggle, volume + mute for clips with audio, controls hide after 2 s without mouse movement (§2.9)
- [ ] Volume persists across clips and launches; HAP audio plays in sync (§2.9)
- [ ] Alpha preview over checkerboard or black; global default toggle plus per-preview B key; DXV alpha follows the clip's Alpha Type live (§2.9)
- [ ] Window title shows `<filename> [codec] [LOOP] — W×H · fps · duration` while previewing (§2.9)
- [ ] Thumbnail prefetch pauses while a preview is open (§2.4)

### Drag and system integration
- [ ] Dragging a selected clip drags the whole selection, an unselected clip only itself; the drop target (Resolume Arena, file manager, editor) receives real file paths (§2.10)
- [ ] Drag starts after 5 px of movement and never starts a marquee (§2.10)
- [ ] Context menu, in order: Reveal, Open With ▸, Copy Path, —, Show in Glance, Send to GlEnc, Send to Arena ▸, —, Rename…, Generate / Regenerate Brief (§2.18)
- [ ] Context-menu actions apply to the whole selection when invoked on a selected clip, else to that clip only (§2.18)
- [ ] Open With list is fetched off the UI thread and cached; scrolling never triggers a lookup; a slow network share does not stall the UI (§2.11, `crate/CLAUDE.md:233-235`)
- [ ] Copy Path puts one path, or newline-joined paths, on the clipboard (§2.10)

### Send to Arena
- [ ] With Arena reachable, the submenu lists New Layer at Top, each layer (top first) with Next Free Column, Append as New Column and one item per column showing empty / clip name / playing mark (§4.7)
- [ ] Loading one clip into an empty slot results in `GET` of that slot returning the clip, state `Disconnected` (§4.6)
- [ ] Loading over an occupied slot asks Replace / Skip Occupied / Cancel and honours each answer (§4.6)
- [ ] Multi-selection fills consecutive columns in the chosen sort order; missing columns are appended (§4.6)
- [ ] New Layer adds a layer at the top and loads from column 1 (§4.6)
- [ ] Trigger After Load is off by default; when on, exactly the first loaded clip is triggered (§4.6)
- [ ] Paths with spaces, `#`, `%`, `&`, `?` and non-ASCII characters load correctly (§4.2)
- [ ] Default update mode On Demand: opening the menu fetches at most once per 2 s and shows "Updated n s ago" + Refresh; Live and Interval selectable; the choice persists (§4.5)
- [ ] Arena not running / webserver off / unreachable host each show their own one-line message in the menu, and no retries run while a local Arena is not running (§4.5, §4.8)
- [ ] A layer deleted or reordered in Arena between menu and click is detected ("That layer no longer exists") instead of loading into the wrong layer (§4.6)
- [ ] Recent targets (last 5) appear at the top and survive restart (§4.6)
- [ ] All tests run against a scratch layer that is removed by id afterwards; never against the user's layers (§5)

### Brief
- [ ] Generate Brief for one clip from its cached thumbnail; regenerate asks first; missing key opens the key sheet; missing thumbnail and API failure have their own messages (§2.13)
- [ ] Bulk brief for the folder or the current matches with count and cost estimate, confirmation, progress banner with Cancel, one request per distinct content, fan-out to copies without a brief (§2.13)
- [ ] The API key is stored in the OS credential store, never in a file or log (§2.13)
- [ ] Import Briefs from JSON (three shapes) and CSV, with pre-flight counts and the overwrite option; sidecars are written for every affected clip (§2.13)

### Measurements
- [ ] "Scan unmeasured" and "Scan selected" run with progress and Cancel, one scan per distinct content, existing readings never replaced, only one scanner at a time across processes (§2.14)
- [ ] The scan skip list (path fragments) is editable in Settings and honoured (§2.14)
- [ ] Readings for the lab-parity corpus match the Swift scanner's values (loop score, cycle, join step, cuts, flashes, flicker, flow, black share, centre share) (§2.14, `crate/CLAUDE.md:219-223`)
- [ ] Measured box in the info panel shows every row, with "not measured" for missing values and a field-visibility menu (§2.14)
- [ ] `measurements.sqlite` written by either app is read by the other (db §c.3, §d.3)

### Settings and state
- [ ] Every preference in §6.2 has an equivalent with the same default
- [ ] A Mac build taking over an existing install reads the existing roots, exclusions, grid and preview preferences, presets, Alpha Type choices and Send to Arena settings without asking the user to set up again (§6, db §h.4)
- [ ] Closing the window quits the app; launching re-scans all roots using the cache (§2.20)
- [ ] About shows the dependency list with licences; Acknowledgements shows full third-party licence texts; every linked package is listed (§2.15, `crate/CLAUDE.md:266-268`)

### Performance
- [ ] Scrolling a folder of several thousand clips does not run the sort, the context-menu builder, or any OS "which apps open this file" lookup (§2.19)
- [ ] Hover feedback updates only the hovered cell (§2.19)
- [ ] Launch with a 18,000-clip library (the size the code comments cite, `crate/Sources/CrateKit/MetadataStore.swift:372-374`, `:393`) reaches a browsable grid before hashing or backfill finishes
- [ ] No media-framework call during indexing (the Swift app avoids it to prevent the media-library permission prompt) (§2.3)
- [ ] File-system event bursts in a watched folder (about 2 per second) do not rebuild an open menu or reload the grid (`crate/CHECKPOINT.md:18-19`)

### Database compatibility with iCrate (and with the Swift MaCrate)
- [ ] `clip_id` is bit-identical to Swift's for empty, sub-MiB, exactly-1-MiB and larger files (db §e.6)
- [ ] A sidecar written by the Rust app decodes with Swift's `ClipMetadata` decoder: required keys `clip_id`, `created_at`, `updated_at` present; `null` for unset optionals; numbers for timestamps (db §h.1)
- [ ] Sidecar file name is `.` + full clip file name + `.crate-meta.json`, in the clip's directory (db §h.1)
- [ ] A sidecar written by iCrate (including one that holds only `clip_id` and defaults) is read by the Rust app, and its newer `updated_at` wins over the row on the next scan (db §g.4)
- [ ] Round trip: rate and tag a clip in the Rust app → open the drive in iCrate → search finds it, strip shows it → edit in iCrate → Rust app shows the edit after mount/refresh (db §i.4)
- [ ] `.crate-stashes.json` written by the Rust app decodes in iCrate (all keys present, camelCase, `clips` order preserved); a stash created or reordered in iCrate is ingested by the Rust app; entries for clips not on this machine are preserved on write-back (db §h.2)
- [ ] The Rust app never writes keys other than the documented ones into either JSON file (the Swift apps drop unknown keys on save) (db §g.5)
- [ ] Sidecar and stash files are written atomically (temp file + rename in the same directory) (db §h.1, §h.2)
- [ ] `library.sqlite` created by the Rust app opens in the Swift app with no migration re-run: all twelve identifiers present in `grdb_migrations`, schema equal after normalisation (db §c, §d, §i.3)
- [ ] `library.sqlite` created by the Swift app at 0.16.4 opens in the Rust app; every table reads back with the same values and SQLite storage classes (db §i.2)
- [ ] A pre-v8 library upgraded by the Rust app equals the same library upgraded by the Swift app (db §i.3)
- [ ] Foreign keys are on for every connection; renaming a row moves its tags; deleting a row removes its tags and its search row (db §b.3, §f.1)
- [ ] The search table holds exactly one row per clip row after any sequence of upsert / rename / delete / brief fan-out; FTS5 integrity check passes (db §f.2, §i.3)
- [ ] Timestamps are stored as REAL Unix seconds and `is_orphan` as 0/1 integers (db §e.1, §e.3)
- [ ] Journal mode stays the rollback journal; no `-wal` / `-shm` files appear (db §b.3)
- [ ] `measurements.sqlite` next to the library is attached as `meas`, created if missing, and upgraded in place if it lacks `provenance` / `scan_skips` (db §c.3)
- [ ] Presets, Alpha Type choices and orphan dismissals written by one app are honoured by the other (db §f.4)
- [ ] An unchanged iCrate build (0.15.0) passes its own manual checks against a drive last touched only by the Rust app (db §i.4)

---
