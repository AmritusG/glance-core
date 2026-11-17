# v0.6.0 — the show output (for GlancEhr)

Brief: GlancEhr's goal brief (`~/claude-handoffs/GlancEhr_2026-11-17_goal-build_brief.md`, HOW TO WORK 3: core needs are
made here, tagged, pinned there). ROADMAP §2 / §3 named it: "an app-owned output window on its own wgpu surface and
pacing in glance-ui (never an egui viewport), borderless fullscreen on a chosen monitor".

## What shipped

- `glance-ui::output`: a second window with its own surface, Fifo pacing and a blit of the app's picture.
  `App::output(gpu, &OutputFrame) -> Option<wgpu::Texture>` is called once per output refresh (paced by that
  screen). `Host::{screens, open_output, close_output, output_screen, set_cursor_hidden}`; events
  `ShellEvent::{Screens, OutputClosed(OutputEnd), OutputKey}`. Screens are looked at once a second; an output whose
  screen leaves closes with `OutputEnd::ScreenGone`; `find_screen` matches a remembered screen by id, else by name
  (a replug renumbers it on macOS).
- `glance-platform` (macOS): `screen_names()` (NSScreen's names by display id: winit only says "Monitor #n"),
  `set_show_window()` (level 1000, above the menu bar, Dock and notifications; every Space; not hidden on deactivate),
  `set_cursor_hidden()` (NSCursor hide/unhide, balanced). On macOS the output is an undecorated window over the screen
  at that level: winit's borderless full screen put its own level back and left the menu bar over the show.
- Breaking (minor bump): three `ShellEvent` variants; `glance-sample` matches them.

## Validated

- Gates on the home machine (GLANCE_TESTFILES, GLANCE_REQUIRE_GPU=1): fmt, clippy -D warnings, tests all ok, deny ok,
  `cargo check` for aarch64-apple-ios and x86_64-apple-darwin ok. The Windows check runs in CI (a Mac has no MSVC for
  zstd-sys).
- Live, in GlancEhr on a virtual second screen (CGVirtualDisplay, GlancEhr's `tools/virtual-screen.swift`): sent at
  launch to the remembered screen, closed on unplug, sent again on replug (new display id, found by name), full
  screen with no menu bar (GlancEhr `docs/handoffs/w1-*.md`).

## Addendum: unattended running (GlancEhr wave 3)

- `glance-platform::unattended`: `StayAwake` (macOS IOPM `PreventUserIdleDisplaySleep`; Windows `PowerCreateRequest` +
  `PowerSetRequest` display + system), `login_item` / `set_login_item` (macOS `SMAppService.mainApp`; Windows the
  per-user Run key, `StartupApproved` read, never written). A new file, so it does not touch `macos.rs` work in flight.
- Measured from GlancEhr: `pmset -g assertions` lists "GlancEhr is playing a show"; a test bundle registered (On) and
  unregistered (Off). Windows: `cargo clippy -p glance-platform --target x86_64-pc-windows-msvc` clean here; the Windows
  CI job builds it.
- Seen while gating: `glance-library`'s `test_media_reads_as_swift_reads_it` fails on this Mac (182 files, expects 183)
  with or without this change: for the v0.6.0 (MaCrate) wave.

## Addendum: decode on Windows and iOS (GlancEhr wave 5)

- DXVKit Plus pinned by commit 59695e2 (0.14.0: the Media Foundation reader `dxvkit-plus-windows`, on top of MaCrate's
  0.13.0); `glance-media` picks Media Foundation on Windows and the Apple backend on iOS as well as macOS;
  `glance-gpu`'s native import builds for iOS too. Windows frames come in as RGBA8 planes (DXVKit's `CpuFrame`), so
  glance-gpu needs no Windows code.
- Gates: as before; `cargo check --target aarch64-apple-ios` ok; the Windows build is CI's (zstd-sys needs MSVC).
  The same glance-library parity test fails here as before (media count), unrelated.
