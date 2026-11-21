# glance-core CHECKPOINT

Wave: scrub-paths-and-guard. Brief: ~/claude-handoffs/glance-core_2026-11-21_scrub-paths-and-guard_brief.md. Machine: the core's home machine. Previous waves (v0.8.1 and before): closed; their checkpoint is in git history.

## DONE
- DIAGNOSE: fresh mirrors of glance-core, DXVKit-Plus, glenc-public scanned (every blob, tree path, commit and tag message, ref name). Real hits: glance-core 64 (home path 3, temp dir 4, host name 55, session id 2), DXVKit-Plus 280 account name / 19 home paths / 214 scratch paths / 1 temp dir / 2 host name, glenc-public 1 (a session URL in the message of the v1.3.0 commit). The four apps are private: not rewritten.
- Safety bundles (all refs, verified, test-restored): ~/claude-handoffs/backups/<repo>/<repo>_pre-scrub_2026-11-21.bundle for the three.
- Rewrite on mirrors (scratch ~/Documents/claude/scrap-test/glance-core/rewrite, rules in tools/ there), in the order DXVKit-Plus, glance-core, glenc-public so each Cargo.lock names the new commits of what it pins: every real count 0, gitleaks clean, only docs/, CHECKPOINT.md and Cargo.lock differ at any ref.
- The guard was written and tested in scratch first (work/glance-core: tools/path-guard.sh, path-guard-setup.sh, push-main.sh, .gitleaks.toml, the path-guard workflow, kit checks, the rule in CLAUDE.md / README / kit README; work/DXVKit-Plus and work/glenc-public likewise): selftest ok, old histories 128 / 1078 / 2 hits, cleaned histories 0, the four installed apps and three released DMGs clean.

- Published 2026-11-21: DXVKit-Plus and glance-core deleted and re-created from the cleaned mirrors (all refs, same names, description, topics, public, secret scanning on); glenc-public force-pushed (Amrit's choice: stars and releases kept, its 8 old commits stay reachable by id until GitHub collects them). Old commit ids: 62 / 62 and 52 / 52 answer HTTP 422, web 404; events feeds empty; unauthenticated ls-remote equals the cleaned mirrors (`docs/data/scrub/public-check.txt`).
- Local checkouts of glance-core and DXVKit-Plus replaced by clones of the clean repos (old ones in the Bin).
- The guard in glance-core: `tools/path-guard.sh` (hook, gate, CI job, kit check of the bundle and of the DMG's contents), `tools/path-guard-setup.sh`, `tools/push-main.sh`, `.gitleaks.toml`, `.github/workflows/path-guard.yml`, rule 11 in CLAUDE.md, the README and the kit README. Gates 459 / 0, path-guard clean.

## NEXT
- Push this commit with tools/push-main.sh; the same guard commit in DXVKit-Plus (committed locally) and glenc-public (scratch clone ~/Documents/claude/scrap-test/glance-core/pub/glenc-public); branch protection (path-guard required, admins included) on the three; the proof (hook refuses a made-up path; the CI job fails on a throwaway branch made through the API); the four apps (locks refreshed, gates running: commit, then tools/path-guard.sh + the gate line + the rule); handoff.

## STATE
- glance-core, DXVKit-Plus and glenc-public are PUBLIC; the old history is only in the private bundles (~/claude-handoffs/backups/<repo>/<repo>_pre-scrub_2026-11-21.bundle). Never push a bundle.
- Gates on the home machine: set GLANCE_TESTFILES to the test media folder, GLANCE_REQUIRE_GPU=1.
- main is pushed with tools/push-main.sh (rule 11, CLAUDE.md "Pushing").

## OPEN
- [ ] The guard commit pushed in the three public repos, branch protection set
- [ ] The proof (hook refusal, CI failure on a throwaway branch, a clean commit passing)
- [ ] The four apps: lock refresh committed and pushed, hook, gate line, rule
- [ ] Handoff

## IDEAS
- Windows: Media Foundation reader / writer behind glance-encode's platform module (with DXVKit Plus's Windows backend).
