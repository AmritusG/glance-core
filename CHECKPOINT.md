# glance-core CHECKPOINT

Wave: scrub-paths-and-guard. Brief: ~/claude-handoffs/glance-core_2026-11-21_scrub-paths-and-guard_brief.md. Machine: the core's home machine. Previous waves (v0.8.1 and before): closed; their checkpoint is in git history.

## DONE
- DIAGNOSE: fresh mirrors of glance-core, DXVKit-Plus, glenc-public scanned (every blob, tree path, commit and tag message, ref name). Real hits: glance-core 64 (home path 3, temp dir 4, host name 55, session id 2), DXVKit-Plus 280 account name / 19 home paths / 214 scratch paths / 1 temp dir / 2 host name, glenc-public 1 (a session URL in the message of the v1.3.0 commit). The four apps are private: not rewritten.
- Safety bundles (all refs, verified, test-restored): ~/claude-handoffs/backups/<repo>/<repo>_pre-scrub_2026-11-21.bundle for the three.
- Rewrite on mirrors (scratch ~/Documents/claude/scrap-test/glance-core/rewrite, rules in tools/ there), in the order DXVKit-Plus, glance-core, glenc-public so each Cargo.lock names the new commits of what it pins: every real count 0, gitleaks clean, only docs/, CHECKPOINT.md and Cargo.lock differ at any ref.
- The guard was written and tested in scratch first (work/glance-core: tools/path-guard.sh, path-guard-setup.sh, push-main.sh, .gitleaks.toml, the path-guard workflow, kit checks, the rule in CLAUDE.md / README / kit README; work/DXVKit-Plus and work/glenc-public likewise): selftest ok, old histories 128 / 1078 / 2 hits, cleaned histories 0, the four installed apps and released DMGs clean.

- Published 2026-11-21: DXVKit-Plus and glance-core deleted and re-created from the cleaned mirrors (all refs, same names, description, topics, public, secret scanning on); glenc-public force-pushed (Amrit's choice: stars and releases kept, its 8 old commits stay reachable by id until GitHub collects them). Old commit ids: 62 / 62 and 52 / 52 answer HTTP 422, web 404; events feeds empty; unauthenticated ls-remote equals the cleaned mirrors (`docs/data/scrub/public-check.txt`).
- Local checkouts of glance-core and DXVKit-Plus replaced by clones of the clean repos (old ones in the Bin).
- The guard in glance-core: `tools/path-guard.sh` (hook, gate, CI job, kit check of the bundle and of the DMG's contents), `tools/path-guard-setup.sh`, `tools/push-main.sh`, `.gitleaks.toml`, `.github/workflows/path-guard.yml`, rule 11 in CLAUDE.md, the README and the kit README. Gates 459 / 0, path-guard clean.

- Guard pushed: glance-core e1a7f3f and a0e59b1 (through tools/push-main.sh), DXVKit-Plus 1289460 and eb023fc, glenc-public ebb60c5 and 1d3593c; the path-guard job green on each.
- Branch protection on main of the three: path-guard required, admins included, no force push, no deletion (`docs/data/scrub/protection.txt`).
- Proof (`docs/data/scrub/guard-proof.txt`): the hook refused a throwaway commit with a made-up home path; the CI job failed on a throwaway branch (run 44918764269, branch deleted); GitHub refused a straight push to main; clean commits passed.
- The four apps: locks on glance-core v0.8.1 = 83f5bcd and DXVKit Plus v0.15.1 = 899fb50, gates 21 / 71 / 234 / 195 passed, 0 failed (`docs/data/scrub/gates.txt`); the guard copy, the gate line and the rule; pushed (glance-rs dbe81e2, cueglance-rs 4d3ae88, glenc-rs 09f1f78, macrate-rs c1d6e0e).
- What has shipped: four installed apps and four released DMGs clean under the guard (`docs/data/scrub/shipped-check.txt`).
- Handoff: `docs/handoffs/scrub-paths-and-guard.md`. Wave closed 2026-11-21.
- 2026-11-21 (GlancEhr session, all-functions-and-transitions): **v0.9.0** — glance-ui several outputs at once
  (add / close per screen, `OutputFrame::screen`), glance-platform `SyphonServer` (macOS, Syphon.framework loaded at
  run time, wgpu's MTLTexture handed over) and `SpoutSender` (Windows, Spout 2's sender protocol); Handoff docs/handoffs/v090-several-outputs-syphon-spout.md.

- 2026-11-21 (GlancEhr session, crawl-eval-transitions): **v0.9.1** — glance-ui's headless stage as a scripted host
  (input events, AccessKit tree, made-up screens, outputs by the shell's rules, quit), glance-encode `FrameWriter`,
  DXVKit Plus v0.15.2 (a CVMetalTextureCache crash); Handoff docs/handoffs/v091-stage-host-frame-writer.md.

## NEXT
- Nothing queued for this wave. Amrit: remove the delete_repo scope if wanted; decide on the installed Glance Sample 0.3.1 (holds the home path; never published); GitHub Support can purge glenc-public's old commits. Next core work: the Windows milestone (ROADMAP §7).

## STATE
- glance-core, DXVKit-Plus and glenc-public are PUBLIC; the old history is only in the private bundles (~/claude-handoffs/backups/<repo>/<repo>_pre-scrub_2026-11-21.bundle). Never push a bundle.
- Gates on the home machine: set GLANCE_TESTFILES to the test media folder, GLANCE_REQUIRE_GPU=1; `tools/path-guard.sh check` is a gate.
- main is protected: push with tools/push-main.sh (rule 11, CLAUDE.md "Pushing"). Tags: git push origin <tag>.
- A changed tools/path-guard.sh or .gitleaks.toml goes to glance-core main first, then tools/path-guard-setup.sh and a commit in every other repo (DXVKit-Plus's and glenc-public's CI compare their copy with glance-core's).

## OPEN

## IDEAS
- Windows: Media Foundation reader / writer behind glance-encode's platform module (with DXVKit Plus's Windows backend).
