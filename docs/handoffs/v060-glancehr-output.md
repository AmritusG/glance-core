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
