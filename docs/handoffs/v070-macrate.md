# v0.7.0 — MaCrate's core (glance-library, glance-arena, glance-platform library calls)

Brief: macrate-rs's goal brief (HOW TO WORK 4: core needs are made here, tagged, pinned there). Tag order: v0.7.0
contains GlancEhr's v0.6.0 work (de5830d, ef44d35 are ancestors); v0.6.0 is GlancEhr's to tag, on an ancestor of
v0.7.0 only (Cowork answer, 2026-11-18).

## What shipped (since v0.5.0, MaCrate's part)

- `glance-library`: MaCrate's database (GRDB's twelve migrations, byte-equal schema text), sidecars, stashes,
  `clip_id`, header reader, enumeration, roots (ef44d35); indexer, FSEvents batch filter, hashing service, duplicate
  registry, root store and folder sets, folder tree, thumbnails (9b6bd1f); `brief_import` (eaec6b3); exactly rounded
  JSON numbers so 17-digit sidecar timestamps read as Foundation reads them (f0b4b42).
- `glance-arena`: the Send to Arena client from SendEhrKit (MIT); an open's read-back waits for Arena to place the clip
  (ce11c66, fixed on purpose: SendEhrKit 1.1.2 counted loaded clips as failed).
- `glance-media`: DXVKit Plus re-exports and v0.13.0 native facts / frames / thumbnail grab (948c3d9, 9399f48).
- `glance-platform`: plain bookmarks, NSWorkspace calls, Copy Path, Keychain, FolderWatch, file drag (ef44d35);
  `recognise_text` (0119944); app-defined Edit-menu items keep their enabled state under AppKit validation (50eeb14,
  4120178); `date_added`.
- `glance-audio`: with loop off the heard position reaches the end (342fcd8).

## Fixed for the tag (CI had been red since 9b6bd1f)

- Windows check: `examples/indexer_twin.rs` made a symbolic link with `std::os::unix` unconditionally; now
  `#[cfg(unix)]`, as `write-fixtures.rs` does.
- macOS test `hashing::a_cancelled_session_returns_what_it_completed` (64 of 64 completed on the runner): the workers
  ran ahead of the calling thread through an unbounded channel, so a cancel from `progress` came after every hash was
  done. Now a rendezvous channel: a worker takes its next path only after the calling thread took its result, as
  Swift adds the next task only after `group.next()` returned one. The test bounds the completed count by
  `1 + 2 × CONCURRENT_HASHES`.
- Workspace version 0.7.0.

## Validated

- Gates on the home machine with `GLANCE_TESTFILES` and `GLANCE_REQUIRE_GPU=1`: fmt, clippy -D warnings, tests all ok
  (the test-media enumeration case GlancEhr saw at 182 / 183 passes), deny ok, release build ok. The hashing tests
  eight times in a row ok. The Windows check runs in CI (no MSVC tools here for the C build scripts).
- Tag made only after CI on this commit is green.

## Addendum after the tag: the kit (tools/package, no crate change)

- `bundle.sh`: rustc's source paths remapped (`--remap-path-prefix` for the cargo home → `/cargo`, the repo → `/src`,
  the stage's target → `/target`); a binary that still holds `$HOME` is refused. Measured on MaCrate 0.17.0: 1,895
  cargo-home paths (panic locations of registry / git crates) before, 0 after; notarised fine. Apps that copied the
  kit earlier (Glance 0.19.0 ships 1,422 such paths) take it at their next release.
- `lib.sh`: optional `DMG_NAME` slot, for a releases repo with its own naming (MaCrate: `MaCrate-<version>.dmg`).
