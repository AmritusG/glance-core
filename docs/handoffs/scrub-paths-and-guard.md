project:    glance-core
pass:       scrub-paths-and-guard
STATUS:     COMPLETE
parent:     f3e0ebd (glance-core main before the rewrite; no longer on GitHub, kept in the private bundle)
result:     glance-core ae2bedc (rewritten main), e1a7f3f, a0e59b1 + this commit; DXVKit-Plus 899fb50 (rewritten), 1289460, eb023fc; glenc-public 60d6b98 (rewritten), ebb60c5, 1d3593c; glance-rs dbe81e2; cueglance-rs 4d3ae88; glenc-rs 09f1f78; macrate-rs c1d6e0e
session:    4404a933-f8be-4dea-ae25-0e7bb7f1c38d
written_at: 2026-11-21T16:00Z

# scrub-paths-and-guard: the public histories cleaned of build-machine paths, and a guard that refuses the next one

Brief: `~/claude-handoffs/glance-core_2026-11-21_scrub-paths-and-guard_brief.md`. Amrit: "Clean it out and make it an
IMPOSSIBLE TO BREAK RULE, please!" (2026-11-21 15:18). His three answers during the pass: grant the delete permission;
glenc-public by force-push, keeping the repo; the author e-mail in commits stays.

## What was done

1. **Diagnose** (fresh mirror clones; every blob, file name, commit and tag message, ref name). Real hits:
   glance-core: home path 3, temporary folder 4, host name 55, session id 2. DXVKit-Plus (the rows overlap: one line
   can hold several): account name 280, home paths 19, session scratch paths 214, temporary folder 1, host name 2.
   glenc-public: 1, a session link in the message of the v1.3.0 commit; its files
   were clean). The four app repos are private and were not rewritten. Counts: `docs/data/scrub/scan.txt`.
2. **Safety bundles** of the three, all refs, verified and test-restored, private:
   `~/claude-handoffs/backups/<repo>/<repo>_pre-scrub_2026-11-21.bundle`; beside each, the rules its rewrite used
   and the old → new commit map of every commit.
3. **Rewrite** (git filter-repo, blobs and messages): home paths → `<home>` / `<workspace>/` / `<testfiles>`, the
   account name → `<user>`, temporary and scratch folders → `<tmp>`, the host name → `<host>`, session links and ids →
   `<session>`. Done in the order DXVKit-Plus, glance-core, glenc-public, and each `Cargo.lock` in history was given the
   new commit ids of the repos it pins, so every tag still resolves. After: every real count 0 in all three; the guard's
   own scan of every ref clean (before: 128 / 1078 / 2 hits); gitleaks clean. At every ref only `docs/`,
   `CHECKPOINT.md` and `Cargo.lock` differ from the old tree; no source file. (Between refs, four glance-core commits
   that pinned DXVKit Plus by `rev` in `Cargo.toml` got the new id there too.)
4. **Published.** DXVKit-Plus and glance-core deleted and re-created (same name, description, topics, public, wiki
   setting, secret scanning and push protection on), main and every tag pushed under the same names (DXVKit-Plus's two
   explore branches too). glenc-public force-pushed: its 3 stars and 8 releases (6 with DMGs) are kept.
   Checks (`docs/data/scrub/public-check.txt`): unauthenticated `ls-remote` equals the cleaned mirrors (14 / 23 / 10
   refs); old commit ids: glance-core 62 of 62 and DXVKit-Plus 52 of 52 answer HTTP 422, web 404; both events feeds
   were empty after the re-create and hold no old id (they now list this pass's own pushes). Old → new commit of every ref: `docs/data/scrub/tagmap.txt` (glance-core v0.8.1 = 83f5bcd, v0.8.0 = c3f7a49;
   DXVKit-Plus v0.15.1 = 899fb50, v0.15.0 = 1a31112; glenc-public v1.7.0 = 60d6b98).
5. **Local checkouts** of glance-core and DXVKit-Plus: the old folders are in the Bin, clones of the clean repos are in
   their place. (glenc-public has no checkout on this machine.)
6. **The four apps**: `cargo update` of the glance-core and DXVKit Plus packages moved only their commit ids
   (v0.8.1 → 83f5bcd, v0.15.1 → 899fb50); gates pass (glance-rs 21 / 0, cueglance-rs 71 / 0, glenc-rs 234 / 0,
   macrate-rs 195 / 0; `docs/data/scrub/gates.txt`); committed and pushed.
7. **The guard.**
   - `tools/path-guard.sh` (bash starts Perl; one file): home paths on macOS, Windows and Linux, also as folder names;
     temporary and scratch folders; volumes; host names; session links; `ls -l` owner columns; this machine's own
     account name, home folder and host names wherever they stand; and the account name as a SHA-256, so the check
     also works on a CI runner without the name being in the repo. A name is a hit unless it is on the short lists of
     made-up names tests and docs use. Modes: pre-push hook (every commit the remote lacks: author, message, every
     file added or changed, whole; tag messages; ref names), `check` (the gate: hook installed + working tree),
     `installed`, `tree`, `commits`, `history`, `files` (binaries too), `selftest`.
   - The hook is installed in all seven repos (`tools/path-guard-setup.sh` copies the script and installs it); the
     gates of each repo now hold `tools/path-guard.sh check` (public) or `installed` (apps).
   - CI job `path-guard` in the three public repos: the guard over every ref on every push and pull request, gitleaks
     with the same rules (`.gitleaks.toml`), and in DXVKit-Plus and glenc-public a check that their copy is
     glance-core's. It is a **required status check on `main` with no bypass for admins** in all three
     (`docs/data/scrub/protection.txt`); force pushes and deletion of `main` are off. A commit reaches `main` through
     `tools/push-main.sh` (to `guard/<sha>`, the job, then `main`).
   - The kit: `bundle.sh` runs the guard over the finished `.app`, `dmg.sh` over the mounted contents of the image.
   - Proof (`docs/data/scrub/guard-proof.txt`): the hook refused a throwaway commit with a made-up home path (exit 1,
     nothing reached origin); the CI job failed on a throwaway branch holding it (run 44918764269; branch deleted);
     GitHub refused a new commit pushed straight to `main` ("Required status check path-guard is expected"); clean
     commits went in through `tools/push-main.sh`.
   - What has shipped, under the full guard: the four installed apps and their four released DMGs are clean
     (`docs/data/scrub/shipped-check.txt`).
8. **The rule**, word for word: glance-core `CLAUDE.md` (rule 11), `README.md` and the kit README; DXVKit-Plus
   `CONTRIBUTING.md`; glenc-public `README.md`; the four apps' `CLAUDE.md` (rule 8).

## Deviations and notes

- **glenc-public was force-pushed, not re-created** (Amrit's choice). Its 8 old commits still answer by id (HTTP 200)
  until GitHub collects them, and its events feed still names two of them (the push of 2026-11-17); the session link
  in the old v1.3.0 commit opens only with Amrit's login. GitHub Support can purge them on request. Secret scanning is
  off on glenc-public, as it was before (on for the two re-created repos, as before).
- **`Cargo.lock` changed in history** (glance-core at all 14 refs, glenc-public at 2), beyond the brief's "docs only":
  without it no old tag would resolve its dependencies. Checked: every pin at every ref exists in the re-created repo;
  `cargo metadata --locked` on glenc-public's main passes.
- **The apps got more than the lock commit**: one commit with the guard copy, the gate line and the rule, and (glance-rs,
  cueglance-rs, glenc-rs) one more with the guard's later copy; only `CLAUDE.md` and `tools/path-guard.sh`. The brief's
  header says "lock refresh only", its step 6b asks for the hook and a gate in every repo. No app code changed; no app was rebuilt
  or released. Released apps record core revisions (glance-core 1bf280f, DXVKit Plus 4946175) that now exist only in
  the private bundles; the same tags carry the same source.
- **The apps' gate is `installed`, not `check`**: their private histories and trees still hold old paths (not in this
  brief). The hook scans whole files, so the first commit that touches such a file has to scrub it.
- **What machinery cannot do**: `git push --no-verify` skips a local hook; on the public repos the required CI job
  still stops the commit at `main`, on the private repos only the rule stands. Tags are not covered by branch
  protection (the hook scans them; the CI job runs on them and reports). The throwaway proof commit 7b38bef (a made-up
  path) stays on GitHub unreferenced until collected.
- **DXVKit-Plus has no CLAUDE.md**; the rule is in its CONTRIBUTING. Its cargo gates were not re-run (no Rust source
  changed); `cargo fmt --all --check` and the guard pass; its Windows workflow is green on eb023fc.
- **glenc-public's guard commit came from a scratch clone.** The next re-seed from glenc-rs must keep
  `tools/path-guard.sh`, `tools/push-main.sh`, `.gitleaks.toml`, `.github/workflows/path-guard.yml` and the README's
  Rules, and push with `tools/push-main.sh` (note left for glenc-rs).
- **The kit's new steps were not run inside a full release** (none in this pass): `kit_path_guard` and `dmg.sh`'s own
  lines were run on an installed app, a file with a made-up path and a released image (`docs/data/scrub/kit-check.txt`).
  The apps' copies of the kit get them at their next kit copy.
- **Found, not fixed**: the installed `Glance Sample.app` 0.3.1 (the throw-away kit proof, never published, built
  before the kit remapped source paths) holds the home path 2507 times (the guard prints 5014: it reads a binary a
  second time as UTF-16 and counts plain text twice; pass or fail is not affected).
- The first CI run on the re-created glance-core main (ae2bedc) was cancelled by the next push (the workflow's
  concurrency rule); the run on a0e59b1 is the one to read.
- cc-kit's checkpoint line "Machine: <host>" and its `ls -la` receipts are hits under this rule; this file gives sizes
  without the owner column (note left for cc-kit).
- Scratch (48 MB, with the full gate logs and the scratch clone of glenc-public) went to the Bin before this file was
  committed; `cargo clean` in glance-core and the four apps freed 7328 MB by cargo's own count (2.0 + 1.2 + 1.2 + 1.2 +
  1.5 GiB). The Bin cannot be listed from a session; the two old checkouts and the scratch were moved with `trash`.
- **Not checked by the independent verifier** (cc-kit:verifier confirmed the rest: published refs, a fresh full scan of
  what GitHub serves, all old ids, protection, CI, locks, gates from the logs, hooks, the rule's text, bundles, tree
  diffs): the DMG scans (the images are in the kit's stage folders under `$TMPDIR/glance-kit/`, which it did not
  search), the Bin, the 7328 MB, and that the re-created repos have the old settings (compared here with what
  `gh repo view` printed before the delete: descriptions, DXVKit-Plus's 9 topics and wiki on, glance-core no topics
  and wiki off, secret scanning on).

## Needs Amrit

- The gh token still has `delete_repo`: `gh auth refresh -h github.com -r delete_repo` takes it away.
- `Glance Sample.app` in /Applications: delete it, or say so and it is rebuilt with the current kit.
- glenc-public's old commits: ask GitHub Support to purge them, or leave them to GitHub's own collection.
- The Bin holds the two old checkouts (old history): empty it when satisfied.

## RECEIPTS

```
$ scan after the rewrite (docs/data/scrub/scan.txt; tools/path-guard.sh history on each cleaned mirror)
glance-core:  path-guard: clean - every ref: commits, files, tag messages, ref names (2127 objects, 13230046 bytes)   before: 128 hits
DXVKit-Plus:  path-guard: clean - every ref: commits, files, tag messages, ref names (1959 objects, 13739125 bytes)   before: 1078 hits
glenc-public: path-guard: clean - every ref: commits, files, tag messages, ref names (968 objects, 5286343 bytes)     before: 2 hits
gitleaks 8.30.1 (default rules + .gitleaks.toml): 62 / 53 / 11 commits scanned, no leaks found

$ public check (docs/data/scrub/public-check.txt)
glance-core:  created 2026-11-21T15:12:27Z PUBLIC; unauthenticated ls-remote SAME as the cleaned mirror (14 refs); 62 old commits: 62 x HTTP 422; events feed: 0 old ids
DXVKit-Plus:  created 2026-11-21T15:12:00Z PUBLIC; unauthenticated ls-remote SAME as the cleaned mirror (23 refs); 52 old commits: 52 x HTTP 422; events feed: 0 old ids
glenc-public: created 2026-06-19T23:17:16Z PUBLIC stars 3; ls-remote SAME (10 refs); 8 old commits: 8 x HTTP 200 (kept repo); releases v1.6.2:2 v1.6.1:2 v1.6.0:2 v1.5.0:2 v1.4.0:2 v1.3.0:2 v1.2.0:0 v1.1.0:0
web, unauthenticated: glance-core/commit/1bf280f 404, glance-core/commit/e2d5582 404, DXVKit-Plus/commit/4946175 404

$ branch protection (docs/data/scrub/protection.txt)
glance-core: required path-guard strict=false enforce_admins=true force_pushes=false deletions=false
DXVKit-Plus: required path-guard strict=false enforce_admins=true force_pushes=false deletions=false
glenc-public: required path-guard strict=false enforce_admins=true force_pushes=false deletions=false

$ the guard's proof (docs/data/scrub/guard-proof.txt)
$ git push origin throwaway/guard-proof     (commit 99f35bb)
commit 99f35bb:guard-proof.txt:1: [home path] /Us...(12 characters)
path-guard: 1 hit in the 1 commits of this push (6 objects, 219 bytes). Fix each at its source; nothing is waved through.
exit 1; on origin: 0 refs named throwaway/guard-proof
path-guard check on 7b38bef: completed failure        (run 44918764269, branch made through the API, then deleted)
$ git push origin main     (commit a0e59b1)
remote: error: GH006: Protected branch update failed for refs/heads/main.
remote: - Required status check "path-guard" is expected.
 ! [remote rejected] main -> main (protected branch hook declined)
$ tools/push-main.sh
==> path-guard passed; a0e59b1 to main
push-main: origin/main is a0e59b1

$ gates (docs/data/scrub/gates.txt)
glance-core: fmt ok, clippy ok, test ok (459 passed, 0 failed, 0 ignored; GLANCE_TESTFILES set, GLANCE_REQUIRE_GPU=1), deny ok (advisories ok, bans ok, licenses ok, sources ok), build ok, path-guard check clean (1318 objects)
glance-rs: GATES-OK | tests 21 passed, 0 failed
cueglance-rs: GATES-OK | tests 71 passed, 0 failed
glenc-rs: GATES-OK | tests 234 passed, 0 failed
macrate-rs: GATES-OK | tests 195 passed, 0 failed
path-guard selftest: 17 lines that must hit, 17 that must not, 5 of this machine's names: ok

$ app heads and locks (all pushed, main...origin/main in sync)
glance-rs dbe81e2 lock: DXVKit-Plus v0.15.1#899fb50 glance-core v0.8.1#83f5bcd
cueglance-rs 4d3ae88 lock: DXVKit-Plus v0.15.1#899fb50 glance-core v0.8.1#83f5bcd
glenc-rs 09f1f78 lock: DXVKit-Plus v0.15.1#899fb50 glance-core v0.8.1#83f5bcd
macrate-rs c1d6e0e lock: DXVKit-Plus v0.15.1#899fb50 glance-core v0.8.1#83f5bcd

$ hooks (tools/path-guard-setup.sh)
glance-core: hook installed / DXVKit-Plus: hook installed / glance-rs: hook installed / cueglance-rs: hook installed / glenc-rs: hook installed / macrate-rs: hook installed / glenc-public (scratch clone): hook installed

$ what has shipped (docs/data/scrub/shipped-check.txt)
/Applications/Glance.app 0.19.2, CueGlance.app 1.5.2, GlEnc.app 1.8.0, MaCrate.app 0.17.2: path-guard: clean
Glance-v0.19.2.dmg, CueGlance-v1.5.2.dmg, GlEnc-v1.8.0.dmg, MaCrate-0.17.2.dmg, mounted: path-guard: clean

$ private copies (bytes, name; ~/claude-handoffs/backups/)
2564419 glance-core/glance-core_pre-scrub_2026-11-21.bundle     sha256 aa02a8f7752c6175...
2576372 DXVKit-Plus/DXVKit-Plus_pre-scrub_2026-11-21.bundle     sha256 8ee37c0331175369...
1229913 glenc-public/glenc-public_pre-scrub_2026-11-21.bundle   sha256 ca348a76a944fd36...
each: refs digest of the restored clone = the mirror's, git fsck clean; beside each: <repo>_2026-11-21_commit-map.txt and the rules of its rewrite

$ re-run after the verifier's report
glenc-public 1d3593c: cargo metadata --locked: exit 0
DXVKit-Plus eb023fc: cargo fmt --all --check: exit 0

$ CI (main)
glance-core 44919093384 CI a0e59b1 completed success (lint, macos, windows); 44919093388 path-guard a0e59b1 success
DXVKit-Plus 44919138032 Windows eb023fc success; 44919137605 path-guard eb023fc success
glenc-public 44919163831 path-guard 1d3593c success
glance-rs 44919170969 CI dbe81e2 success; cueglance-rs 44919173688 CI 4d3ae88 success; glenc-rs 44919177251 CI 09f1f78 success; macrate-rs 44919187872 CI c1d6e0e success

$ git log -1 of this file's own commit and its push: in the mailbox copy
```
