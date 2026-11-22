# glance-core CHECKPOINT

Wave: serde-float-roundtrip (v0.9.4), run from the glance-rs session. Brief: ~/claude-handoffs/glance-rs_2026-11-22_serde-float-roundtrip-check_brief.md (from TestKard, TESTKARD-025). Machine: the core's home machine. Previous wave (covered-window-pacing / v0.9.3) closed; its checkpoint is in git history.

## DONE
- READ: every Cargo.toml for serde_json; the JSON read back with doubles: glance-settings FileStore (`real` values; the Windows store, and MaCrate's crawl mode), glance-arena's shared store (Swift `Date`s in recents.json / cache.json), glance-library's sidecars (already had float_roundtrip on its own line). glance-proof / glance-sample only write JSON.
- MUST-FAIL: crates/glance-settings/tests/float_roundtrip.rs and crates/glance-arena/tests/float_roundtrip.rs (typical values / dates the crate writes plus 200,000 seeded random doubles, bit for bit, and a byte-identical re-save). With the feature off: 39 837 of 200 006 and 25 750 of 200 001 read back off (docs/data/serde-float-roundtrip/must-fail.txt).
- FIX: serde_json's `float_roundtrip` on the workspace dependency (every crate inherits it; glance-library's own feature line folded into it). No file format and no Cargo.lock entry changed. Both tests pass.
- Workspace version 0.9.4 (version + lock).
- Gates (docs/data/serde-float-roundtrip/gates.txt): fmt, clippy, deny, build, path-guard green; tests 472 passed / 1 failed (v0.9.3: 468 / 3): the one failure is files_swift_parity, clipS_ProRes 4444.mov still not in the test media folder.

## NEXT
- Push through tools/push-main.sh, CI green, tag v0.9.4, push the tag.
- No app needs v0.9.4 now: MaCrate (the only user of FileStore on macOS and of glance-arena) already builds serde_json with float_roundtrip from its own workspace; Glance, CueGlance and GlEnc keep their settings in UserDefaults. Apps pick v0.9.4 up at their next pin bump.

## STATE
- Other sessions push to glance-core main from their own clones (GlancEhr's did, v0.9.0): `git fetch` and compare with origin/main before committing a version or tagging.
- glance-core, DXVKit-Plus and glenc-public are PUBLIC; the old history is only in the private bundles (~/claude-handoffs/backups/<repo>/<repo>_pre-scrub_2026-11-21.bundle). Never push a bundle.
- Gates on the home machine: set GLANCE_TESTFILES to the test media folder, GLANCE_REQUIRE_GPU=1; `tools/path-guard.sh check` is a gate.
- main is protected: push with tools/push-main.sh (rule 11, CLAUDE.md "Pushing"). Tags: git push origin <tag>.
- A changed tools/path-guard.sh or .gitleaks.toml goes to glance-core main first, then tools/path-guard-setup.sh and a commit in every other repo (DXVKit-Plus's and glenc-public's CI compare their copy with glance-core's).
- The kit's temporary folder is KIT_TMP (lib.sh): physical ${TMPDIR:-/tmp}. A record of the app's id under it whose path is gone is the kit's own (ls-cleanup.sh).

## OPEN

## IDEAS
- Windows: Media Foundation reader / writer behind glance-encode's platform module (with DXVKit Plus's Windows backend).
- Kit: dmg.sh's mount point in its own folder, away from the staging copy (runs E and I of kit-dmg-record-fix register nothing): removes the cause; the trap's unregister then has nothing to do. Or an unregister straight after the guard's detach, so the record does not live through the notarise wait.
- Kit: macOS 27 prints a deprecation warning for `hdiutil create / attach / detach` ("use diskutil image ..."); move when it stops working or earlier.
