project:    glance-core
pass:       kit-dmg-record-fix
STATUS:     COMPLETE
parent:     8eaa92e (v0.9.0; the work started on eadb97f, see Deviations)
result:     this commit, tagged v0.9.1 (its id and push: in the mailbox copy)
session:    17b3f85d-efd4-4381-b8c2-ebbfd024984f
written_at: 2026-11-21T18:10Z

# kit-dmg-record-fix: dmg.sh no longer leaves a LaunchServices record for its staging copy (kit at v0.9.1)

Brief: `~/claude-handoffs/glance-core_2026-11-21_kit-dmg-record-fix_brief.md`. Source: the note from glenc-rs
(GlEnc 1.8.1's release ended with a record for the kit's deleted staging copy, so `ls-cleanup.sh` and `install.sh`
ended with "WARNING: expected exactly one record"). Kit only: no crate code changed, no app rebuilt or released.

## What was done

1. **Read**: the note, `tools/package/{dmg,ls-cleanup,release,install,lib}.sh`, `lsregister -h`.

2. **Diagnose** (`docs/data/kit-dmg-record/diagnose.txt`). Test app: glance-sample built by the kit under a
   throw-away id (`com.amritus.glancecore.kitrecordtest`, "Kit Record Test"), signed, notarised and stapled, staged in
   scratch; the records of that id counted in `lsregister -dump` before and after every step, and again 8 to 12 s later.

   | Run | Steps | Record for the staging copy |
   |---|---|---|
   | A | `ditto` into a `mktemp -d` folder, wait, delete | none |
   | B | `ditto` + `hdiutil create -srcfolder`, never mounted | none |
   | C | the mount alone (a folder that never held a copy), guard, detach | none |
   | D | dmg.sh's order | none after ditto / create / attach / guard; **1 after the detach**; still there after `rm -r` |
   | D2 | as D without the guard, mounted 12 s+ | none after the attach (also 12 s later); **1 after the detach** |
   | E | staging copy in one folder, mount point in another | none |
   | F | staging copy deleted before the mount | none |
   | I | copy in `<work>/src`, mount at `<work>/mnt` | none |
   | G | attach and detach at once (nothing in between) | none |
   | H | `lsregister -u` while mounted (no record yet: exit 1), then detach | **1 after the detach** |
   | J1 to J3 | dmg.sh's order, `lsregister -u <copy>` straight after the detach | exit 0 each time (the record is there when detach returns); none afterwards, also 12 s later |
   | K | `lsregister -u` by path of a record whose folder is gone | exit 0, record removed |

   So: **neither the `ditto` nor the mount registers the copy. The detach does**, of an image whose mount point sits in
   the same folder as the staging copy; the record is for the copy beside the mount point, not for the mounted one.
   (In run G, attach and detach with nothing in between, nothing was registered: one run, no durations taken, so
   "only when mounted for a while" is a guess the fix does not rely on.) That is exactly what eadb97f's dmg.sh added (the guard's mount at
   `<staging>/mnt`). Why LaunchServices does it was not looked into; the fix does not depend on it.
   Also seen in the session (counts taken by hand, not in the evidence files, which start after it): the staged build
   product had no record after `bundle.sh` and `sign.sh` and one after `notarize.sh` (its Gatekeeper / staple step);
   that one was always `ls-cleanup.sh`'s to remove.

3. **Fix, both ways.**
   - `dmg.sh`: one `cleanup` on EXIT, on success and on failure: detach the image if it is still mounted (plain, then
     `-force`), `lsregister -u` the staging copy and the mounted copy by path (non-zero when there is no record: not an
     error), then delete the folder. Order matters (run H: unregistering before the detach is too early). The guard
     call lost its own detach-and-exit (the trap does it). The staging folder is made under `KIT_TMP`, physical path.
   - `ls-cleanup.sh`: a record `KIT_TMP/*/<APP_NAME>.app` whose path is no longer on disk is the kit's own and is
     unregistered ("unregistered the kit's deleted staging copy"); one that is still on disk is left alone and listed.
   - `lib.sh`: `KIT_TMP` = `${TMPDIR:-/tmp}` as its physical path (LaunchServices records `/private/var/…`).
   - Kit README: the two rows.

4. **Prove** (`docs/data/kit-dmg-record/prove.txt`; the kit's sha256 there equals the committed files).
   - Full `dmg.sh` run of the test app (create, guard over the mounted image, sign, notarise Accepted, staple, sha256),
     exit 0: records before 1 (the staged build product), after 1 (the same): **0 for staging paths**, 0 staging folders
     left on disk, 0 images left mounted. The guard ran: `path-guard: clean - mnt (8 objects, 32171064 bytes)`; and over
     the bundle in `bundle.sh`: `path-guard: clean - Kit Record Test.app (4 objects, 32091851 bytes)`.
   - `install.sh` into `<scratch>/install` (not /Applications): **"OK: exactly one record"**, exit 0.
   - Failure after the detach (a notary profile that does not exist), fixed dmg.sh: exit 69, 0 staging records. The old
     dmg.sh (eadb97f) in the same run: 1 staging record left.
   - Failure while mounted (a stand-in guard that waits 6 s and reports a hit; dmg.sh byte-identical), fixed dmg.sh:
     exit 1, 0 staging records, image not left mounted, folder gone. The old dmg.sh in the same run: 1 more left.
   - On those two stale records: the old `ls-cleanup.sh` says "left alone" twice and "WARNING: expected exactly one
     record", exit 1 (the note's symptom, reproduced); the new one unregisters both and says "OK: exactly one record",
     exit 0.
   - Afterwards: the test app unregistered (0 records of the test id), the scratch folder moved to the Bin
     (0 records after the move too), `cargo clean` in the repo.

5. **Commit, push, tag, notes**: workspace version 0.9.1 (only the version and the lock's 12 workspace entries; no
   dependency moved); pushed with `tools/push-main.sh`; tag `v0.9.1` (not v0.8.2: see Deviations); one note per app
   in the mailbox (glance-rs, cueglance-rs, glenc-rs, macrate-rs). No app pin was changed.

## Deviations and notes

- **The tag is v0.9.1, not the brief's v0.8.2.** GlancEhr's session had pushed v0.9.0 (8eaa92e: several outputs,
  SyphonServer, SpoutSender) from its own clone at 16:46Z and tagged it at 17:02Z, before the brief was written
  (17:30Z); this clone had not fetched and still stood on eadb97f, and the brief names v0.8.1 as the newest. The first `tools/push-main.sh` was refused by GitHub (not a
  fast-forward; nothing was forced, the guard branch it had made was deleted). The commit was rebased onto v0.9.0
  (conflicts only in the version, the lock and the checkpoint), the gates run again on that tree, and the next patch
  from main is v0.9.1: a tag comes from a green main, and main is past 0.9.0. The kit's files are byte-identical to
  the ones proven (v0.9.0 does not touch `tools/`). The apps take the kit as files (`cp -Rp`), so the fix does not
  ask them to move their core pin to 0.9.x.
- **The test app is not the sample's own id.** `ls-cleanup.sh` must end with one record, and the sample's id belongs to
  a real (if throw-away) app; a separate id and a plist without document types keep the test from ever touching it or
  becoming a handler for anything. Built with `CARGO_PROFILE_RELEASE_LTO=off` and 16 codegen units (the kit's own
  override): the build's speed is not what was measured.
- **Two uploads to Apple's notary service** under the stored profile, both of the throw-away app (the `.app` zip, then
  the DMG of the full `dmg.sh` run; submissions eb8eb08f… and 681c623e…), both Accepted. The brief's "full dmg.sh run" needs a stapled app and notarises the
  image. Nothing was published or installed outside scratch; nothing in /Applications was touched.
- **The controls ran the old kit** (`git archive eadb97f`) only in failing runs (no third upload).
- **Not built: moving the mount point away from the staging copy.** Runs E and I register nothing at all, so a mount
  point in its own folder would remove the cause instead of cleaning up after it. The brief names the two ways above
  and "nothing else"; with them in place the third would make the trap's unregister a no-op. Amrit's call (IDEAS in
  the checkpoint).
- **`hdiutil create / attach / detach` print a deprecation warning on macOS 27.0.1** ("use diskutil image …"). They
  still work; the kit is unchanged there. It will show in every release log (IDEAS).
- A stale record made by eadb97f's kit before this fix (any app released with it: so far GlEnc 1.8.1, already
  unregistered by hand) is removed by the new `ls-cleanup.sh` at the next `release.sh` / `install.sh`.
- `tools/path-guard.sh` is unchanged, so nothing to propagate with `path-guard-setup.sh`; the apps take
  `tools/package/` again (`cp -Rp`) at their next release.
- The evidence files were scrubbed (`<TMPDIR>`, `<scratch>`, `<home>`, `<volume>` for the test image's mount path) and
  pass `tools/path-guard.sh files`.

## Independent check (cc-kit:verifier, after the tag)

50 claims re-derived from the files, the Bin's raw logs, git and GitHub: 37 confirmed, 9 unproven, 4 wrong. The four
wrong ones were statements in this write-up, not in the kit, and are corrected here and in `diagnose.txt`'s header:
v0.9.0 was on main before the brief was written (not "while the pass ran"); run G's printed heading; "a dump takes
15 s" (3.4 s when it timed it); the scratch is 2.5 GB, not 2.6. Unproven by it: the reading of the files (no
artefact); the local gates (now `docs/data/kit-dmg-record/gates.txt`; CI on ed843aa shows the same 459 / 0 / 0
without the test media); the text of the two refused pushes (in the session only; quoted in the mailbox copy); the
by-hand counts above; the number of notary uploads (it did not use the stored profile); `cargo clean`'s size; and
"/Applications untouched": no command of this pass wrote there and the kit ran with a scratch `INSTALL_DIR`, but it
saw `GlEnc.app`'s ctime change at 18:28 local and `OneDrive.app`'s mtime at 18:24, which it could not attribute
(this pass did not touch either).

Its reading of the kit diff: no blocking bug; nothing can unregister or delete the installed copy or a bundle still on
disk. Its notes, all low: (1) the staging copy's record lives from the guard's detach until `dmg.sh` exits (the
sign / notarise wait), because the unregister is in the trap; the staged build product's own record lives through the
same wait and until `ls-cleanup.sh`, so this adds no new exposure, and an unregister straight after the detach or the
mount point in its own folder would close it (IDEAS). (2) The trap relies on the record being there when detach
returns (measured 4 of 4); a later one would be caught by `ls-cleanup.sh` at the next release. (3) The two
`lsregister -u` calls are silent. (4) `ls-cleanup.sh`'s pattern matches at any depth under the temporary folder (only
paths that are gone are unregistered). (5) `KIT_TMP` follows `TMPDIR`: a run with another `TMPDIR` leaves a stale
record "left alone". (6) `kit_load` now stops every kit script when `TMPDIR` names a missing folder. (7) If both
detaches fail the trap's `rm -r` fails on the read-only mount and replaces the exit code, as the old trap would.

## Needs Amrit

- Nothing blocks. The scratch (about 2.5 GB, mostly the test build's target) is in the Bin as
  `glance-core-scratch_kit-dmg-record_2026-11-21`: empty the Bin when convenient.
- Optional decisions: the mount point in its own folder (cause removed); `diskutil image` instead of `hdiutil`.

## RECEIPTS

```
$ gates on the commit's tree = v0.9.0 + this fix (GLANCE_TESTFILES set, GLANCE_REQUIRE_GPU=1)
cargo fmt --all --check: exit 0
cargo clippy --workspace --all-targets --release -- -D warnings: Finished `release` profile, exit 0
cargo test --workspace --release: exit 0; 50 "test result: ok" lines, 459 passed, 0 failed, 0 ignored (the same count on eadb97f + this fix, before the rebase)
cargo deny check: advisories ok, bans ok, licenses ok, sources ok
cargo build --release: Finished `release` profile, exit 0
tools/path-guard.sh check: path-guard: clean - the hook is installed; the working tree (1348 objects), exit 0

$ diagnose (docs/data/kit-dmg-record/diagnose.txt), run D: dmg.sh's order
[D after ditto] records: 1
[D after hdiutil create] records: 1
[D after attach] records: 1
[D after the path guard] records: 1
[D after detach] records: 2
[D after rm -r] records: 2
[D after rm -r (+8s)] records: 2
$ run J1 to J3: lsregister -u straight after the detach
lsregister -u straight after detach: exit 0
[J1 after rm -r (+12s)] records: 2
lsregister -u straight after detach: exit 0
[J2 after rm -r (+12s)] records: 2
lsregister -u straight after detach: exit 0
[J3 after rm -r (+12s)] records: 2
(those 2: the staged build product and run D's stale record, removed in run K; none for a J folder)

$ prove (docs/data/kit-dmg-record/prove.txt): the fixed dmg.sh, full run
[before the new dmg.sh] records: 1
[before the new dmg.sh] staging folders left on disk: 0
[before the new dmg.sh] images mounted under the temporary folder or <volume>: 0
path-guard: clean - mnt (8 objects, 32171064 bytes)
    submission 681c623e-9a14-4222-b5ef-33612a918d24: Accepted
dmg.sh exit 0
[after the new dmg.sh (full run)] records: 1
[after the new dmg.sh (full run)] staging folders left on disk: 0
[after the new dmg.sh (full run)] images mounted under the temporary folder or <volume>: 0
$ install.sh into <scratch>/install
==> installed <scratch>/install/Kit Record Test.app (0.8.1)
    unregistered build product: <scratch>/stage/Kit Record Test.app
==> OK: exactly one record (<scratch>/install/Kit Record Test.app)
install.sh exit 0
$ failure paths (A, B: fixed kit; C, D: eadb97f) and ls-cleanup.sh (E: eadb97f; F: fixed)
(records in A and B: the installed and the staged copy, no staging path; C and D each add one <TMPDIR>/T/tmp.*/ record)
##### A: NEW dmg.sh, notarisation refused (no such profile): failure after the detach
exit 69
[A new, failure after detach] records: 2
##### B: NEW dmg.sh, the guard reports a hit: failure while the image is mounted
exit 1
[B new, failure while mounted] records: 2
##### C (control): OLD dmg.sh (eadb97f), notarisation refused
exit 69
[C old, failure after detach] records: 3
##### D (control): OLD dmg.sh (eadb97f), the guard reports a hit
exit 1
[D old, failure while mounted] records: 4
##### E (control): OLD ls-cleanup.sh (eadb97f)
    left alone (not a build product or DMG copy): <TMPDIR>/T/tmp.gQtPyyUoI1/Kit Record Test.app
    left alone (not a build product or DMG copy): <TMPDIR>/T/tmp.qtUeLaUKsF/Kit Record Test.app
WARNING: expected exactly one record (<scratch>/install/Kit Record Test.app).
exit 1
##### F: NEW ls-cleanup.sh
    unregistered the kit's deleted staging copy: <TMPDIR>/T/tmp.gQtPyyUoI1/Kit Record Test.app
    unregistered the kit's deleted staging copy: <TMPDIR>/T/tmp.qtUeLaUKsF/Kit Record Test.app
==> OK: exactly one record (<scratch>/install/Kit Record Test.app)
exit 0
[F after the new ls-cleanup.sh] records: 1

$ afterwards
lsregister -u <scratch>/install/Kit Record Test.app: exit 0; records of com.amritus.glancecore.kitrecordtest: 0
scratch moved to the Bin (glance-core-scratch_kit-dmg-record_2026-11-21, about 2.5G); records of the test id 5 s later: 0

$ evidence files (bytes; gates.txt added with the closing commit)
 11968 docs/data/kit-dmg-record/diagnose.txt
  9191 docs/data/kit-dmg-record/prove.txt
  9222 docs/data/kit-dmg-record/gates.txt

$ git log -1 of this file's own commit, the push, the path-guard job, the tag: in the mailbox copy
```
