# clean-history-and-publish: glance-core's history cleaned, published (MIT); apps re-pinned, CI on, glenc-public re-seeded

Brief: `~/claude-handoffs/glance-core_2026-11-17_clean-history-and-publish_brief.md` (Amrit: "public", 2026-11-17 08:46).
Removal list: the blocked scan `~/claude-handoffs/glance-rs_2026-11-17_make-glance-core-public-result_BLOCKED.md`.

## What was done

1. **Safety first.** Local folder clean and in sync with origin (8e37377). A full bundle of the old history
   (all refs, 32 commits, verified, test-restored) is kept privately in the mailbox backups, never pushed. The research
   notes (`docs/research/`, 6 files) were copied with `cp -p` to a new **private** repo, AmritusG/glance-research
   (verified byte-equal after a fresh clone).
2. **Rewrite** (git filter-repo on a fresh mirror): `docs/research/` and the two C3 window captures
   (`docs/data/c3-look/*.png`, a frame of a test clip) dropped from every commit; 21 ordered regex replacements over
   every blob and every commit message: the test-media folder → `<testfiles>`, scratchpad / temporary / workspace /
   home paths and the account name → placeholders, other local projects → `<another project>`, the claude.ai session
   URLs → `<session>`, third-party clip names → neutral names (`std_film_*`, `clipS_ProRes 4444.mov`). Counts before →
   after: `docs/data/publish/removal-scan.txt` (e.g. account name 322 → 0, test-folder path 140 → 0, third-party names
   89 → 0, the PNGs by hash 2 → 0, session URLs 2 + 5 in messages → 0); gitleaks over all refs: no leaks (33 commits).
3. **The new tip** (4ae1791): tests and `tools/c2-measure.sh` find the media only through `GLANCE_TESTFILES` (unset:
   skip). The 14 rows renamed in `tests/data/swift-glance-testfiles.tsv` are found by their bytes
   (`tests/data/testfiles-by-content.tsv`: size + FNV-1a 64 of the first 64 KiB; each unique among the folder's 199
   files). `docs/research/README.md` says the notes are kept privately. ATTRIBUTION.md, CLAUDE.md, the c3-look README and
   06-C3-RESULTS made true for the cleaned history. `LICENSES/DXVKit-MIT.txt` was already in the tree since C1 (the
   blocked scan's "never committed" was wrong); it is byte-identical to DXVKit v1.0.0's LICENSE.
4. **Published.** Force-pushed main and the 8 tags, made public, then found that GitHub still served the old commits by
   SHA and its public events feed listed them (e.g. the commit that added the PNGs). Made private again after about 3
   minutes (0 forks, 0 stars); Amrit chose **delete + re-create**. The repo was deleted and re-created public with the
   same name and description, and the cleaned refs pushed. Old SHAs now answer "no commit" (API 422, web 404), the
   events feed is empty, unauthenticated `git ls-remote` works (`docs/data/publish/public-check.txt`). The old
   repo's Actions history and its one deploy key went with it.
5. **Local folder**: the old checkout went to the Bin (with `.ref/`, `target/` and the Swift tool's `.build/` moved into
   the new clone first), the new clone took its place; gates pass (`docs/data/publish/gates.txt`).
6. **Apps** (Cargo.toml already pointed at the https URL; same tags): `cargo update -p glance-media` moved only the
   glance-core packages; CI reads the core over https without a key; `.cargo/config.toml` no longer forces the git CLI.
   glance-rs had CI (deploy-key steps removed); cueglance-rs and glenc-rs had it parked in `docs/ci/ci.yml` (moved to
   `.github/workflows/ci.yml`, deploy-key steps removed). The `GLANCE_CORE_DEPLOY_KEY` secret on glance-rs is deleted;
   no secret is left on any of the three.
7. **glenc-public** re-seeded with the Rust source and tagged v1.7.0 (scrub: code and build kit; no tests, examples,
   docs, handoffs, test tools, CI or local paths; a new README pointing at glenc-releases for the DMG). A fresh clone
   from GitHub with an empty CARGO_HOME and no credentials builds (`docs/data/publish/glenc-public-build.txt`).
8. **Amrit's answer recorded**: "public" (2026-11-17 08:46) in `docs/00-ROADMAP.md` §1 and the intro, and in the three
   apps' CHECKPOINT.md (glenc-rs also in its goal report's answers); the Windows-only parity items stay open for the
   Windows milestone.

## Tags (old → new commit; source identical)

| Tag | Old | New |
|---|---|---|
| v0.1.0 | a0c2d04 | 39fde1a |
| v0.2.0 | 2d29f8e | 211e0b1 |
| v0.3.0 | ae8930d | 0ffe8a4 |
| v0.3.1 | 871d0f7 | 50a469b |
| v0.3.2 | 8c90e4d | 5a4d700 |
| v0.3.3 | 6816383 | ea3374f |
| v0.4.0 | 0e8cb32 | 46b53e7 |
| v0.5.0 | 65dfda7 | 49fdbe8 |

main: 8e37377 → b187b6e (rewritten) + 4ae1791 (the publish tip). For v0.3.3, v0.4.0 and v0.5.0 no file under
`crates/*/src`, `Cargo.*` or `.cargo` differs between old and new; only docs, test data and test support changed.

## Apps

| App | Pin | Lock (old → new) | Gates | CI |
|---|---|---|---|---|
| glance-rs (1f20339) | v0.3.3 | 6816383 → ea3374f | 19 passed / 0 failed | 44087710651 green (lint, macos, windows) |
| cueglance-rs (3261caf) | v0.4.0 | 0e8cb32 → 46b53e7 | 69 / 0 | 44088181135 green (lint, macos, windows) |
| glenc-rs (8605aa5) | v0.5.0 | 65dfda7 → 49fdbe8 | 232 / 0 | 44131258624: lint, windows green; **macos cancelled 4 times, never started** ("not acquired by Runner of type hosted") |

## Deviations and notes

- **OPEN: glenc-rs's macOS CI job never got a runner.** Four attempts (16:55–18:08 UTC) each queued 15 minutes and
  were cancelled by GitHub ("The job was not acquired by Runner of type hosted even after multiple attempts"; "Due to
  capacity constraints, jobs targeting macOS arm64 runners may experience longer queue times"). In the same window the
  public glance-core's macOS job (run 44140024833) ran and passed, and earlier glance-rs and cueglance-rs (private) did
  too. GitHub status: all operational. So it looks like a limit on macOS runners for private repositories on this
  account (Actions minutes or quota; reading the usage needs the `user` scope). The workflow is the same as
  cueglance-rs's; glenc-rs's gates pass locally on macOS (232 / 0). Amrit: check Settings → Billing → Actions usage,
  then re-run (`gh run rerun 44131258624 --failed -R AmritusG/glenc-rs`).

- **Delete + re-create** instead of a plain visibility change (Amrit's choice, above). The brief's force-push and
  visibility steps were done first; the material was public for about 3 minutes before the repo went private again.
- **glenc-rs had no v1.7.0 tag.** The release build is 2e4625a; after it only `#[cfg(test)]` code changed, so
  glenc-public was seeded from glenc-rs 8605aa5 (the refreshed lock is needed to build from the public core) and the
  tag v1.7.0 was made on glenc-public only.
- **Released apps are not rebuilt.** Glance 0.19.0, CueGlance 1.5.0 and GlEnc 1.7.0 record core revisions (6816383,
  0e8cb32, 65dfda7) that now exist only in the private backup bundle; the same tags on the new commits carry the same
  source.
- Product names (MaCrate, GlancEhr, SendEhr), the notary profile names and private repo names outside the research
  notes were kept: they are roadmap and kit level, and the blocked scan judged the kit clean. The internals it listed
  (team id, app group, keychain names, the lab path) were only in the research notes, now removed.
- Gates on the home machine now need `GLANCE_TESTFILES` set for the media tests to run (CLAUDE.md says so).
