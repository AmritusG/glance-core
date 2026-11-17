#!/bin/sh
# The files cross-test: Swift MaCrate's own code (FilesOracle, built by files-oracle.sh) and glance-library
# (crates/glance-library/examples/files_twin.rs) run on the same inputs, compared line by line with diff. Prints
# one count line per check; never prints a test-media file name. Results: docs/data/m1/files-crosstest.txt.
#
#   tools/swift-library-oracle/files-crosstest.sh <macrate-source-dir> <work-dir> [<test-media-dir>]
#
# Inputs: the generated folder and movies (files_twin tree / headers), the committed input columns of
# crates/glance-library/tests/data/files/, glance-core's tests/fixtures, and (read only) the test media.
set -eu
[ $# -ge 2 ] || { echo "usage: $0 <macrate-source-dir> <work-dir> [<test-media-dir>]" >&2; exit 2; }
here=$(cd "$(dirname "$0")" && pwd)
core=$(cd "$here/../.." && pwd)
work=$2
media=${3:-}
oracle=$("$here/files-oracle.sh" "$1" "$work")
(cd "$core" && cargo build --release -p glance-library --example files_twin >&2)
twin="$core/target/release/examples/files_twin"
data="$core/crates/glance-library/tests/data/files"
run="$work/run"
[ -d "$run/tree/noperm" ] && chmod 755 "$run/tree/noperm"
rm -rf "$run"
mkdir -p "$run/in"

# Both sides run with HOME (and CFFIXED_USER_HOME, which Foundation reads first) in the work folder, so
# MaCrate's CrateDiag log, UserDefaults and Application Support resolve there, never into the user's own.
home="$work/home"
mkdir -p "$home"
compare() { # name, mode, argument(s)…
    name=$1; shift
    HOME="$home" CFFIXED_USER_HOME="$home" "$oracle" "$@" >"$run/$name.swift"
    HOME="$home" CFFIXED_USER_HOME="$home" "$twin" "$@" >"$run/$name.rust"
    printf '%s: %s lines, %s differing\n' "$name" "$(wc -l <"$run/$name.swift" | tr -d ' ')" \
        "$(diff "$run/$name.swift" "$run/$name.rust" | grep -c '^>' || true)"
}

cut -f1 "$data/swift-doubles.tsv" >"$run/in/doubles"
cut -f1 "$data/swift-paths.tsv" >"$run/in/paths"
cut -f1 "$data/swift-clipsize.tsv" >"$run/in/clipsize"
cut -f1 "$data/swift-downscale.tsv" >"$run/in/downscale"
compare doubles doubles "$run/in/doubles"
compare names sort "$data/names-in.txt"
compare paths paths "$run/in/paths"
compare clipsize clipsize "$run/in/clipsize"
compare downscale downscale "$run/in/downscale"

"$twin" tree "$run/tree"
"$twin" headers "$run/headers"
tree=$run/tree
compare tree-enum enum "$tree"
compare tree-enum-excluded enum "$tree" "$tree/excluded" "$tree/sub/inner"
compare tree-linkdir enum "$tree/linkdir"
compare tree-package-root enum "$tree/Thing.app"
compare tree-sub enum "$tree/sub"
chmod 755 "$tree/noperm"
compare tree-files files "$tree"
compare headers files "$run/headers"
mkdir -p "$run/fixtures"
cp -p "$core"/tests/fixtures/*.mov "$core"/tests/fixtures/*.mp4 "$run/fixtures/"
cp -pR "$core/tests/fixtures/frame-timing" "$core/tests/fixtures/dxvkit" "$run/fixtures/"
compare fixtures files "$run/fixtures"
if [ -n "$media" ]; then
    compare media-files files "$media"
    compare media-enum enum "$media"
fi
echo "differences, if any: diff $run/<name>.swift $run/<name>.rust"
