# glance-core CHECKPOINT

Wave: v050-encode (minor, version 0.5.0), made by the GlEnc goal run. Brief: ~/claude-handoffs/glenc-rs_2026-11-17_goal-build-to-swap_brief.md (HOW TO WORK 4). Machine: Mac17,6. Previous wave: v040-editor-shell (closed, tag v0.4.0 on 0e8cb32; its checkpoint is in git history at 7f0b82e).

## DONE
- glance-encode (new) + shell additions (DroppedAll, open_movies, reveal_in_finder); DXVKit Plus pin v0.12.0. Details: docs/handoffs/v050-encode.md.
- Gates: fmt ok; clippy -D warnings ok; GLANCE_REQUIRE_GPU=1 tests 89 passed 0 failed; deny ok; build ok.
- Docs: CLAUDE.md layout, ROADMAP (crate table, v0.5.0 line, DXVKit pin), PARITY (writers straight alpha ticked).

## NEXT
- Commit, push, CI green, annotated tag v0.5.0; glenc-rs pins v0.5.0.

## STATE
- glenc-rs drives this release; its CHECKPOINT holds the goal's state.

## OPEN
- [ ] commit, push, CI green, tag v0.5.0

## IDEAS
- Windows: Media Foundation reader / writer behind glance-encode's platform module (with DXVKit Plus's Windows backend).
