#!/usr/bin/env bash
# tools/path-guard-setup.sh [<repo folder>...] — put the path guard into the repos of the family.
#
# For each repo (default: every family repo checked out beside this one, this one included):
#   - tools/path-guard.sh there is made this repo's copy (cp -p; a changed copy is yours to commit),
#     and where the repo already has them, .gitleaks.toml and tools/push-main.sh too;
#   - the pre-push hook is installed (.git/hooks/pre-push = the repo's tools/path-guard.sh);
#   - `tools/path-guard.sh installed` must pass.
# Nothing is committed or pushed. Exit 1 when a repo could not be set up.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
family="glance-core DXVKit-Plus glenc-public glance-rs cueglance-rs glenc-rs macrate-rs"
repos=()
if [ $# -gt 0 ]; then
    repos=("$@")
else
    for name in $family; do repos+=("$(dirname "$here")/$name"); done
fi
bad=0
for repo in "${repos[@]}"; do
    name="$(basename "$repo")"
    if [ ! -e "$repo/.git" ]; then echo "$name: not checked out here, skipped"; continue; fi
    repo="$(cd "$repo" && pwd)"
    changed=""
    if [ "$repo" != "$here" ]; then
        mkdir -p "$repo/tools"
        for f in tools/path-guard.sh .gitleaks.toml tools/push-main.sh; do
            if [ "$f" != tools/path-guard.sh ] && [ ! -f "$repo/$f" ]; then continue; fi
            if ! cmp -s "$here/$f" "$repo/$f"; then cp -p "$here/$f" "$repo/$f"; changed="$changed $f"; fi
        done
    fi
    if (cd "$repo" && tools/path-guard.sh install >/dev/null && tools/path-guard.sh installed >/dev/null); then
        echo "$name: hook installed${changed:+; updated:$changed (commit it)}"
    else
        echo "$name: FAILED (run tools/path-guard.sh install there to see why)"
        bad=1
    fi
done
exit $bad
