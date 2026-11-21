# glance-core CHECKPOINT

Wave: kit-dmg-record-fix. Brief: ~/claude-handoffs/glance-core_2026-11-21_kit-dmg-record-fix_brief.md. Machine: the core's home machine. Previous waves (scrub-paths-and-guard, v0.8.1 and before): closed; their checkpoint is in git history. v0.9.0 (GlancEhr session, 2026-11-21: several outputs, SyphonServer, SpoutSender; `docs/handoffs/v090-several-outputs-syphon-spout.md`) reached main from another clone while this wave ran.

## DONE
- READ: the note from glenc-rs, tools/package/{dmg,ls-cleanup,release,install,lib}.sh, `lsregister -h`.
- DIAGNOSE (`docs/data/kit-dmg-record/diagnose.txt`): test app = glance-sample under the throw-away id com.amritus.glancecore.kitrecordtest, built / signed / notarised by the kit in scratch. Neither the ditto nor the attach registers the staging copy: the detach of an image mounted beside it does (runs D, D2, H: 1 record after the detach; A, B, C, E, F, G, I: none). `lsregister -u` straight after the detach: exit 0 three times, none afterwards; by path of a deleted folder: exit 0.
- FIX: dmg.sh's EXIT trap detaches, unregisters the staging copy and the mounted copy, then deletes (success and failure); ls-cleanup.sh unregisters a record under KIT_TMP whose path is gone; lib.sh KIT_TMP (physical $TMPDIR); kit README.
- PROVE (`docs/data/kit-dmg-record/prove.txt`): full dmg.sh run (notarised) exit 0, 0 staging records, guard clean over the mounted image (8 objects); install.sh into scratch: "OK: exactly one record"; failure after the detach and while mounted: 0 staging records (old kit: 1 each); old ls-cleanup WARNING exit 1, new one OK exit 0. Test app unregistered (0 records), scratch in the Bin.
- The first push was refused (not a fast-forward): main had moved to v0.9.0 (8eaa92e, tagged, CI green) from another clone. Rebased onto it; the brief's "next patch (v0.8.2)" is therefore **v0.9.1** (a tag comes from main, and main is past 0.9.0). Workspace version 0.9.1 (version + lock only). The kit's files are the proven ones, unchanged by the rebase.
- Gates on the rebased tree (v0.9.0 + the fix): fmt ok, clippy ok, test 459 passed / 0 failed / 0 ignored (GLANCE_TESTFILES set, GLANCE_REQUIRE_GPU=1), deny ok, build ok, path-guard check clean. Handoff: `docs/handoffs/kit-dmg-record-fix.md`.

## NEXT
- `tools/push-main.sh`, CI green on main, tag v0.9.1 and push it, the four notes and the result in ~/claude-handoffs, `cargo clean`, then close this checkpoint.

## STATE
- Other sessions push to glance-core main from their own clones (GlancEhr's did, v0.9.0): `git fetch` and compare with origin/main before committing a version or tagging.
- glance-core, DXVKit-Plus and glenc-public are PUBLIC; the old history is only in the private bundles (~/claude-handoffs/backups/<repo>/<repo>_pre-scrub_2026-11-21.bundle). Never push a bundle.
- Gates on the home machine: set GLANCE_TESTFILES to the test media folder, GLANCE_REQUIRE_GPU=1; `tools/path-guard.sh check` is a gate.
- main is protected: push with tools/push-main.sh (rule 11, CLAUDE.md "Pushing"). Tags: git push origin <tag>.
- A changed tools/path-guard.sh or .gitleaks.toml goes to glance-core main first, then tools/path-guard-setup.sh and a commit in every other repo (DXVKit-Plus's and glenc-public's CI compare their copy with glance-core's).
- The kit's temporary folder is KIT_TMP (lib.sh): physical ${TMPDIR:-/tmp}. A record of the app's id under it whose path is gone is the kit's own (ls-cleanup.sh).

## OPEN
- [ ] Push, tag v0.9.1, notes to the four apps, mailbox result; cargo clean

## IDEAS
- Windows: Media Foundation reader / writer behind glance-encode's platform module (with DXVKit Plus's Windows backend).
- Kit: dmg.sh's mount point in its own folder, away from the staging copy (runs E and I of kit-dmg-record-fix register nothing): removes the cause; the trap's unregister then has nothing to do.
- Kit: macOS 27 prints a deprecation warning for `hdiutil create / attach / detach` ("use diskutil image ..."); move when it stops working or earlier.
