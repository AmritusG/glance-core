# glance-core

The shared Rust core of the Glance family rebuild (macOS + Windows, egui + wgpu). The apps —
Glance, CueGlance, GlEnc, MaCrate, later GlancEhr — each live in their own repo under
`<workspace>/` and pin this core by git tag. iCrate stays Swift.

Read first: `docs/00-ROADMAP.md` (crates, waves, pins), `docs/01-RESEARCH.md` (what the Swift apps
and the existing Rust code do, with sources), `docs/02-PARITY.md` (the gate each Rust app must pass
before it replaces its Swift twin), `CHECKPOINT.md` (where the work stands). Measured results per wave: `docs/03-W0-RESULTS.md`,
`docs/04-C1-RESULTS.md`, `docs/05-C2-RESULTS.md`, `docs/06-C3-RESULTS.md`.

## Layout

| Crate | What |
|---|---|
| `glance-media` | route a file by content (and say why nothing plays it), open it, the family's alpha rule, the clip key and the Alpha Type hint. A facade over DXVKit Plus. |
| `glance-gpu` | decoded frame → picture (as-stored RGBA from DXVKit Plus's exact block path, then the Alpha Type pass); the device; the 0-difference render harness (`verify`). |
| `glance-playback` | the frame clock: audio master (nearest frame to the sound heard), the 0.25 s wall-clock fallback, Glance's transport rules (seek / step / restart / scrub / loop / end of clip) on DXVKit Plus's transport; a clip's decoded frame behind it (DXVKit Plus cache); timing statistics. |
| `glance-audio` | a clip's first audio track on the output device (cpal; DXVKit Plus decodes, rubato resamples once to the device rate), volume / mute as a gain, the position heard (CoreAudio timestamps). |
| `glance-ui` | the egui host (winit + egui-winit + egui-wgpu, no eframe), the video widget, the checkerboard; the shell: system font, Glance's look and icons, transport bar, scrub row, menus (muda), dialogs (rfd), window rules, `run_shell`; `headless`: the same widget drawn offscreen, for tests. |
| `glance-platform` | the one crate with `unsafe` (own lints, `// SAFETY:` on every block): macOS preferences domain, open-file events, default handler, media keys, recents, subtitle; Windows single instance, settings folder, media controls, menu bar. |
| `glance-settings` | typed settings in the app's store (its `UserDefaults` domain; `%APPDATA%` JSON on Windows) with the Swift apps' keys, encodings and read rules. |
| `glance-sample` | the shell proven in a bundled, notarised app, bundle id `com.amritus.glancecore.sample` (throw-away). Not a product. |
| `glance-proof` | the proof window (W0), and `verify` / `hapq` / `scrub` / `reroute` / `gpu` without a window. Not a product. |

`tools/package/`: the packaging kit (universal build, sign, notarise, DMG, `ls-cleanup`), slots in an `app.conf`.

Planned crates (library-db, arena-client, encode) are in the roadmap; a crate is
created in the wave that fills it, never as an empty stub.

## Rules

1. **One decoder per codec in the whole programme.** Demux, HAP, DXV, LMBD, native OS codecs,
   transport and BC upload are DXVKit Plus's (`AmritusG/DXVKit-Plus`, MIT, pinned by tag in the
   workspace `Cargo.toml`). If the core needs something from a decoder, it goes out as a brief to
   DXVKit Plus; it is never patched or copied here.
2. **No FFmpeg, libav, GStreamer or libmpv** (Amrit's decision). Licences: MIT / BSD / Apache-2.0 /
   Zlib / ISC first; anything else needs Amrit's yes. No GPL / LGPL / MPL code or dependency.
3. **`unsafe` is forbidden in this workspace** (`[workspace.lints]`). Media FFI lives in DXVKit
   Plus's island crates; the app-shell FFI the roadmap names (preferences, open-file events,
   drag-out) live in ONE island crate here, `glance-platform`, with its own lints table and a
   `// SAFETY:` comment on every block. The never-crash lints (`unwrap`, `expect`, `panic`,
   `todo`, `unreachable` denied) apply to all non-test code: every failure is a value.
4. **Other repos are read-only.** `.ref/` holds clones for reading (gitignored). Work for another
   repo leaves as `~/claude-handoffs/<Repo>_<date>_<slug>_brief.md`.
5. **Parity is judged against the Swift source, not its docs.** `docs/research/*.md` lists where
   they disagree; the code wins.
6. **Alpha.** Frames are decoded as stored. DXV alpha is the user's per-clip Alpha Type
   (Premultiplied default / Straight); HAP alpha is straight; the choice is a uniform in the
   present pass, never a re-decode. Writers write straight. The present pass is integer
   arithmetic (one answer on every backend).
7. **Checkerboard** (shared spec): 8 pt squares, greys 0.85 / 0.65, in window points.
8. **Numbers only from release builds.** Test media is Amrit's own, read-only, never copied into
   the repo; say which files. Committed fixtures must be our own (CC0) or MIT.
9. **Nothing is tested on another computer until done.** Development and proofs run on the core's
   home machine; Windows only at the Windows milestone in the roadmap.
10. One `main` branch. Dead code goes. Timestamps preserved on every copy (`cp -p`, `rsync -a`,
    `ditto`).

## Gates (every change)

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --release -- -D warnings
cargo test --workspace --release
cargo deny check
cargo build --release
```

The exactness bar is a test: the picture equals the CPU reference at max 0 (`crates/glance-gpu/tests/exact.rs`),
window-scaled within 1 (`crates/glance-ui/tests/screen.rs`). A change that moves either is not done.
GPU tests skip on a machine without a GPU; on the home machine run them with `GLANCE_REQUIRE_GPU=1`
so a skip is a failure. Tests on Amrit's media skip where the folder is absent (`GLANCE_TESTFILES`
names another one). CI (`.github/workflows/ci.yml`) runs the gates on every push; a tag is made
only from a green `main`. Fixtures and their licences: `tests/fixtures/`, `ATTRIBUTION.md`.

GUI runs: `pgrep -x glance-proof` / `pkill -9 -x glance-proof` before and after (exact name, never
`pkill -f`).

## Handoff (every brief)

Last step: `docs/handoffs/<slug>.md`, committed, then copied to
`~/claude-handoffs/glance-core_<YYYY-MM-DD>_<slug>-result.md` (envelope, receipts and footer per the
`cc-kit:handoff` skill; never overwrite, `-2` if taken). `CHECKPOINT.md` is updated after every
finished step. Chat gets file paths and a few lines, never the handoff body.
