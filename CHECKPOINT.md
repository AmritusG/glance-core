# glance-core CHECKPOINT

Wave: v040-editor-shell (minor, version 0.4.0), made by the CueGlance goal run. Brief: ~/claude-handoffs/cueglance-rs_2026-11-16_goal-build-to-swap_brief.md (HOW TO WORK 4). Machine: <host>. Previous wave: 60hz-and-v033 (closed, tag v0.3.3 on 6816383; its checkpoint is in git history at aa8dd99).

## DONE
- Editor shell: MenuBar / attach_menu_bar / take_chosen / ShellEvent::MenuItem / muda re-export; App::close_requested; install_terminate_guard / set_unsaved_work / PlatformEvent::QuitRequested; ShellConfig::frame_autosave / set_frame_autosave; Host::drop_position / pointer_in_window; open_path; dialog open_file / save_file / pick_folder / choose; finish_app_menus finds Edit by Undo or Copy; kit strip -S. Details: `docs/handoffs/v040-editor-shell.md`.
- Gates `docs/data/v040/gates.txt`: all exit 0, 72 tests (GPU required). Proven in CueGlance RS (`docs/data/v040/quit-guard.txt`, `close-veto.txt`).
- Pushed 0e8cb32; CI run 43983741673 green (lint, macos, windows); annotated tag v0.4.0 → 0e8cb32 pushed. Wave closed.

## NEXT
- Nothing queued here; cueglance-rs pins v0.4.0.

## STATE
- Pins: DXVKit Plus v0.11.0; wgpu 30.0.x, egui 0.36.2, winit 0.30.13, muda 0.21.2. glance-rs stays on v0.3.3 (bump notes in the handoff).

## OPEN

## IDEAS
- Present timestamps from the OS (CAMetalLayer presented time) for the A/V estimate.
- The drop crash (macOS CoreDrag): Apple report sent by Amrit 2026-11-16; no core work owed.
