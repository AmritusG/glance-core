# glance-core CHECKPOINT

Wave: kit-rust-src-remap (v0.9.5). Brief: ~/claude-handoffs/glance-core_2026-11-22_from-glancehr_kit-rust-src-remap.md (from GlancEhr 1.0.4, its commit 6a7bc7b). CLOSED 2026-11-22. Machine: the core's home machine. Previous wave (serde-float-roundtrip / v0.9.4) closed; its checkpoint is in git history.

## DONE
- READ: GlancEhr's tools/package/bundle.sh on its main (gh api, read-only): `rust_src="$("$bin_dir/rustc" --print sysroot)/lib/rustlib/src/rust"` and `--remap-path-prefix=$rust_src=/rustc/src` appended to remap. Not taken: its ENTITLEMENTS-required lib.sh and RESOURCES / FRAMEWORKS slots.
- FOUND: the kit's toolchain (rustup stable 1.100.0) has no rust-src at the moment (components file changed 2026-11-22 18:18, after the brief's 13:56 install); Homebrew's 1.99.1 has it but no x86_64 std, so the kit never uses it. The proof runs on a scrap copy of the toolchain with rust-src-1.100.0 (static.rust-lang.org, sha256 OK) added, KIT_TOOLCHAIN_BIN pointing at it; the shared toolchain is not touched.
- CHANGE: tools/package/bundle.sh (rust_src + remap, comment as GlancEhr's), README's bundle.sh line; workspace version 0.9.5 (version + lock).
- PROOF (docs/data/kit-rust-src-remap/): glance-sample by the kit, rust-src toolchain copy. Before (ead07ca worktree): refused, 82 $HOME paths, all under <toolchain>/lib/rustlib/src/rust/. After: path-guard clean, 0 $HOME, the 82 now /rustc/src/library/... Control (changed kit, rustup stable without rust-src): clean, 0 $HOME.
- GATES (gates.txt): fmt, clippy, deny, build, path-guard green; tests 473 passed / 1 failed (files_swift_parity, clipS_ProRes 4444.mov missing; as v0.9.4).
- HANDOFF: docs/handoffs/kit-rust-src-remap.md. Scrap swept to the Trash (22 378 files, 9.1 GB).

- VERIFIER (cc-kit:verifier): 24 CONFIRMED; UNPROVEN only for what left with the scrap folder (the before run's kit revision, the dropped first attempt, the variables on the --no-fail-fast run; all three named in the handoff) and the then-pending tag / notes / mailbox copy; WRONG only for this file's then-stale NEXT (push already done).
- Pushed 4f0c3d4 through tools/push-main.sh (path-guard green on guard/4f0c3d4eab1c and main); CI on main green: lint, macos, windows (run 45067716481), path-guard (45067716587). Tag v0.9.5 (annotated) on 4f0c3d4, pushed.
- Notes in the mailbox: glance-rs, cueglance-rs, glenc-rs, macrate-rs (`<app>_2026-11-22_note-from-glance-core-kit-rust-src-remap.md`: cp -Rp tools/package/ at the next release; no pin change). GlancEhr already has the change (6a7bc7b).
- Wave closed 2026-11-22. Result: ~/claude-handoffs/glance-core_2026-11-22_kit-rust-src-remap-result.md.

## NEXT
- Nothing queued for this wave. Carried over: clipS_ProRes 4444.mov is still missing from the test media folder (files_swift_parity red on the home machine; Amrit's call). Next core work: the Windows milestone (ROADMAP §7).

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
