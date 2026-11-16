# v0.4.0 — the shell for an editor app (2026-11-17)

Made in glance-core by the CueGlance goal run (brief `~/claude-handoffs/cueglance-rs_2026-11-16_goal-build-to-swap_brief.md`, HOW TO WORK 4: core fixes are made here, tagged, pinned there). Proven in CueGlance RS (com.amritus.cueglance.rs); evidence copied to `docs/data/v040/`.

## What changed
- **App-defined menu bar** (`glance_ui::menu`): trait `MenuBar` (GlanceMenu implements it), `attach_menu_bar` (attach a muda bar the way GlanceMenu does), `take_chosen` / `Chosen` (Glance ids → `ShellEvent::Menu`, any other id → new `ShellEvent::MenuItem(String)`), `pub use muda`. `App::menu` returns `Option<&dyn MenuBar>`. `GlanceMenu::take_commands` is gone (it dropped other ids). `finish_app_menus` finds the Edit menu by Undo **or Copy** (an editor's own Undo item has no `undo:` action).
- **Close veto**: `App::close_requested() -> bool` (default true) on the close button / ⌘W.
- **Quit guard** (`glance_platform`): `install_terminate_guard` adds `applicationShouldTerminate:` to winit's delegate (run_shell installs it; inert until `set_unsaved_work(true)`), answering `NSTerminateCancel` and posting `PlatformEvent::QuitRequested` while the app has unsaved work.
- **Window frame by name**: `ShellConfig::frame_autosave` → `set_frame_autosave` (`NSWindow Frame <name>`, a SwiftUI `Window(id:)` scene's key).
- **Drop position**: `Host::drop_position` (`pointer_in_window` read at the drop).
- `open_path` (Finder opens a file or folder); `dialog::{open_file, save_file, pick_folder, choose}`; `dialog::AlertStyle` re-exported.
- **Kit**: `bundle.sh` runs `strip -S` after `lipo` and refuses when a debug-map entry is left (rustup's rust-objcopy for rustc 1.100 cannot load libLLVM, so Cargo's `strip = "debuginfo"` only warned; Glance 0.19.0 shipped unstripped that way, 3,269 debug-map entries in CueGlance's equivalent build before the fix).

## Validated
- Gates (`docs/data/v040/gates.txt`): fmt, clippy -D warnings, `GLANCE_REQUIRE_GPU=1` tests 72 passed, deny, build; `cargo check` for x86_64-apple-darwin and aarch64-apple-ios ok on this Mac (Windows: CI).
- In CueGlance RS: the muda bar attached with Swift CueGlance's items; a quit Apple Event while clean quits, while dirty shows the question, Cancel keeps the app, Discard quits (`quit-guard.txt`); ⌘W with unsaved cues: Cancel keeps the window, Discard closes (`close-veto.txt`); `NSWindow Frame cue` written; `open_file` opened a movie; the kit's strip: 0 debug-map entries, 27,369 symbols kept.
- Not exercised: `drop_position` (no drops are posted on this machine, the brief's rule 4).

## For glance-rs (when it bumps)
`ShellConfig { …, frame_autosave: None }`; add `ShellEvent::MenuItem(_)` and `PlatformEvent::QuitRequested` arms (or `_`); `menu()` may keep `Some(&self.menu)` with the return type `Option<&dyn glance_ui::menu::MenuBar>`. Re-bundle to get the stripped binary.
