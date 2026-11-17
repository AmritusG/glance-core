# glance-core CHECKPOINT

Wave: clean-history-and-publish. Brief: ~/claude-handoffs/glance-core_2026-11-17_clean-history-and-publish_brief.md. Machine: <host>. Previous wave: v050-encode (closed, tag v0.5.0; its checkpoint is in git history at b187b6e).

## DONE
- DIAGNOSE: local clean and in sync (8e37377); full bundle of the old history kept privately in the mailbox backups (verified, test-restored); research notes copied to the private repo AmritusG/glance-research (byte-equal).
- Rewrite: docs/research/ and the two C3 PNGs dropped from all history; paths, account name, session URLs and third-party clip names replaced in every blob and message; every removal count 0, gitleaks clean (`docs/data/publish/removal-scan.txt`).
- Tip 4ae1791: media through GLANCE_TESTFILES only (renamed rows found by content), docs true for the cleaned history; gates 89 / 0 (`docs/data/publish/gates.txt`).
- Published: force-push, public; old SHAs still served by GitHub → private again (~3 min), Amrit chose delete + re-create → re-created public, cleaned refs pushed; old SHAs 422 / 404, events feed empty, unauthenticated ls-remote ok (`docs/data/publish/public-check.txt`); CI 44087491117 green.
- Local folder replaced by a clone of the new repo (old checkout in the Bin); gates pass.
- Apps re-pinned on the rewritten tags, CI on without a deploy key: glance-rs 1f20339 (CI green), cueglance-rs 3261caf (CI green), glenc-rs 8605aa5 (lint, windows green; macOS never got a runner, below). Deploy-key secret deleted.
- glenc-public re-seeded with the Rust source, tag v1.7.0; clean-room build from a fresh GitHub clone ok (`docs/data/publish/glenc-public-build.txt`).
- Amrit's "public" (2026-11-17 08:46) recorded in ROADMAP and the three apps; Windows-only parity items stay open.
- Handoff: `docs/handoffs/clean-history-and-publish.md`; glance-core CI 44140024833 green on 65c65cc.

## NEXT
- Amrit: Settings → Billing → Actions usage (private-repo macOS minutes), then `gh run rerun 44131258624 --failed -R AmritusG/glenc-rs` and watch to green. Next core work: the Windows milestone (ROADMAP §7).

## STATE
- glance-core is PUBLIC (MIT), re-created 2026-11-17; tags v0.1.0–v0.5.0 on the rewritten commits (table in the handoff).
- Gates on the home machine: set GLANCE_TESTFILES to the test media folder, GLANCE_REQUIRE_GPU=1.
- Never push anything from glance-research or the backup bundle anywhere public.

## OPEN
- [ ] glenc-rs CI 44131258624: macOS job green (4 attempts cancelled by GitHub, "not acquired by Runner of type hosted"; public glance-core's macOS job ran meanwhile; glenc-rs gates 232 / 0 locally)

## IDEAS
- Windows: Media Foundation reader / writer behind glance-encode's platform module (with DXVKit Plus's Windows backend).
