project:    glance-core
pass:       kit-rust-src-remap
STATUS:     COMPLETE
parent:     ead07ca
result:     this commit, tagged v0.9.5 (its id and push: in the mailbox copy)
session:    1902c759-8754-461f-8982-c72632e7b1a7
written_at: 2026-11-22T21:45Z

# kit-rust-src-remap: the kit remaps the toolchain's rust-src folder (kit at v0.9.5)

Brief: `~/claude-handoffs/glance-core_2026-11-22_from-glancehr_kit-rust-src-remap.md` (from GlancEhr 1.0.4, its commit
6a7bc7b). The kit only: no crate code changed, no app rebuilt or released.

## What was done

1. **Read**: GlancEhr's `tools/package/bundle.sh` on its main, read-only through `gh api` (GlancEhr is not checked out
   on the home machine). Its change: `rust_src="$("$bin_dir/rustc" --print sysroot)/lib/rustlib/src/rust"` after
   `cargo_home=`, and `--remap-path-prefix=$rust_src=/rustc/src` added to `remap`.
2. **Change**: the same two lines and the same comment in `tools/package/bundle.sh`. The README's `bundle.sh` line
   now lists the rust-src folder among the remaps. Workspace version 0.9.5 (version and the lock's workspace entries
   only).
3. **Proof**: glance-sample built by the kit three times. The toolchain was a scrap copy of rustup stable 1.100.0 with
   `rust-src-1.100.0.tar.xz` (static.rust-lang.org, sha256 checked) added, passed in `KIT_TOOLCHAIN_BIN`. Evidence:
   `docs/data/kit-rust-src-remap/{before,after,control}.txt`.

   | Run | Kit | Toolchain | Result | `$HOME` in binary | `/rustc/src/library` | `/rustc/<hash>` |
   |---|---|---|---|---|---|---|
   | before | ead07ca (HEAD worktree) | with rust-src | **refused**: "build-machine paths (~) left in the binary: 82" | 82 (all under `<toolchain>/lib/rustlib/src/rust/`) | 0 | 136 |
   | after | this change | with rust-src | `path-guard: clean`, exit 0 | **0** | 82 | 136 |
   | control | this change | rustup stable as installed (no rust-src) | `path-guard: clean`, exit 0 | 0 | 0 | 180 |

   The 82 std paths that leaked in "before" come out as `/rustc/src/...` in "after". In "control" the extra remap
   matches nothing and std keeps its `/rustc/<hash>` paths.
4. **Gates** (`docs/data/kit-rust-src-remap/gates.txt`, GLANCE_TESTFILES set, GLANCE_REQUIRE_GPU=1): fmt, clippy, deny,
   build and path-guard all green. Tests (`--no-fail-fast`): 473 passed, 1 failed. That is the same count as
   v0.9.4's second run. The failure is the known one: `files_swift_parity` finds 182 of 183 files because
   `clipS_ProRes 4444.mov` is still not in the test media folder.

## Deviations and notes

- **The kit's toolchain has no rust-src right now.** The brief says rust-src was installed at 13:56. But rustup
  stable's `lib/rustlib/components` changed again at 18:18 and does not list rust-src, and there is no `src/` folder.
  Homebrew's rustc 1.99.1 has a rust-src folder, but the kit never picks it: it has no x86_64 std. So, unlike the
  brief's "every app gets the refusal now", the kit as it stood would build clean on the home machine today. The
  change is still needed, because rust-src comes back whenever a session adds it.
- To show the refusal without changing the shared toolchain (other sessions build with it), the "before" and "after"
  runs used a throw-away copy with rust-src added. That copy is in the Trash now.
- The first "before" attempt was dropped: `bundle.sh` was edited while that run was reading it. It was rerun from a
  worktree at ead07ca.
- Not taken (per the brief, the kit owner's call): GlancEhr's `lib.sh` requiring the ENTITLEMENTS slot, and its
  RESOURCES / FRAMEWORKS slots. The family default `entitlements.plist` stays as it is.
- Scrap: `~/Documents/claude/scrap-test/glance-core/kit-rust-src`, 22 378 files, 9.1 GB (the toolchain copy and three
  stage targets), moved to the Trash.

## For the apps

Each app's own copy of `tools/package/` takes this at its next release: `cp -Rp` glance-core's `tools/package/` over
the app's copy (`bundle.sh` and `README.md` changed). No pin change. The mailbox has one note per app (glance-rs,
cueglance-rs, glenc-rs, macrate-rs). GlancEhr already has the change (6a7bc7b).
