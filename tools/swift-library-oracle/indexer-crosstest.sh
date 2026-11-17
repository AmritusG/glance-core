#!/bin/sh
# The indexer cross-test: Swift MaCrate's own LibraryIndexer (IndexerOracle, built by indexer-oracle.sh) and
# glance-library's indexer (crates/glance-library/examples/indexer_twin.rs) run the same three passes over the same
# library tree — the launch rebuild (indexRoot per root), FSEvents for the changed folders plus an external sidecar,
# then Refresh / re-include — with the same changes made to the tree between passes; the two library.sqlite files are
# then compared row by row with glance-library's dump (wall-clock created_at / updated_at masked). Also: the watcher's
# parent ascent on the same inputs, and the thumbnail extractor on glance-core's fixtures (and, read only, the test
# media). Results: docs/data/m3/indexer-crosstest.txt.
#
#   tools/swift-library-oracle/indexer-crosstest.sh <macrate-source-dir> <work-dir> [<test-media-dir>]
#
# The tree lives on a scratch APFS disk image mounted at /Volumes/GlanceM3X (the indexer refuses roots under
# /private, /tmp and /var, where scratch folders are). Both sides run with HOME and CFFIXED_USER_HOME in the work
# folder.
set -eu
[ $# -ge 2 ] || { echo "usage: $0 <macrate-source-dir> <work-dir> [<test-media-dir>]" >&2; exit 2; }
here=$(cd "$(dirname "$0")" && pwd)
core=$(cd "$here/../.." && pwd)
mkdir -p "$2"
work=$(cd "$2" && pwd)
media=${3:-}
oracle=$("$here/indexer-oracle.sh" "$1" "$work")
(cd "$core" && cargo build --release -p glance-library --example indexer_twin >&2)
twin="$core/target/release/examples/indexer_twin"
home="$work/home"
mkdir -p "$home"
export HOME="$home" CFFIXED_USER_HOME="$home"

vol=/Volumes/GlanceM3X
if [ ! -d "$vol" ]; then
    [ -f "$work/vol.sparseimage" ] || hdiutil create -size 300m -fs APFS -volname GlanceM3X -type SPARSE "$work/vol" >/dev/null
    hdiutil attach -nobrowse "$work/vol.sparseimage" >/dev/null
fi
pristine=$vol/pristine
lib=$vol/lib
A=$lib/A
B=$lib/B
EX=$A/excluded
rm -rf "$pristine"
"$twin" gen "$pristine"

run=$work/run
rm -rf "$run"
mkdir -p "$run"
printf 'indexRoot\t%s\t%s\nindexRoot\t%s\t%s\nprogress\nsettle\t%s\n' "$A" "$EX" "$B" "$EX" "$EX" >"$run/step1"
{
    for d in "$A" "$A/sub" "$A/moved_deep" "$A/hap" "$A/sub/deep"; do printf 'fsevent\t%s\t%s|%s\t%s\n' "$d" "$A" "$B" "$EX"; done
    printf 'sidecar\t%s\n' "$A/.tiles_dxt1.mov.crate-meta.json"
    printf 'progress\nsettle\t%s\n' "$EX"
} >"$run/step2"
{
    printf 'fsevent\t%s\t%s|%s\t\n' "$A" "$A" "$B"
    printf 'fsevent\t%s\t%s|%s\t\n' "$B" "$A" "$B"
    printf 'indexRoot\t%s\t\n' "$EX"
    printf 'indexRoot\t%s\t\nindexRoot\t%s\t\nsettle\t\nprogress\n' "$A" "$B"
} >"$run/step3"

side() { # name, binary
    rm -rf "$lib"
    cp -Rp "$pristine" "$lib"
    mkdir -p "$run/$1"
    t0=$("$twin" now)
    "$2" run "$run/$1/library.sqlite" "$run/step1" >"$run/$1/out1.txt"
    "$twin" mutate 1 "$lib"
    "$2" run "$run/$1/library.sqlite" "$run/step2" >"$run/$1/out2.txt"
    "$twin" mutate 2 "$lib"
    "$2" run "$run/$1/library.sqlite" "$run/step3" >"$run/$1/out3.txt"
    t1=$("$twin" now)
    echo "$t0 $t1" >"$run/$1/window"
}
side swift "$oracle"
side swift2 "$oracle"
side rust "$twin"
t0=$(cut -d' ' -f1 "$run/swift/window")
t1=$(cut -d' ' -f2 "$run/rust/window")
echo "Swift against itself (a second run): what varies between two Swift runs"
"$twin" compare "$run/swift/library.sqlite" "$run/swift2/library.sqlite" "$t0" "$t1" | grep -E '^(masked|differences)'
echo "Swift against Rust"
for i in 1 2 3; do
    printf 'pass %s events: %s / %s lines, %s differing\n' "$i" "$(wc -l <"$run/swift/out$i.txt" | tr -d ' ')" "$(wc -l <"$run/rust/out$i.txt" | tr -d ' ')" \
        "$(diff "$run/swift/out$i.txt" "$run/rust/out$i.txt" | grep -c '^>' || true)"
done
"$twin" compare "$run/swift/library.sqlite" "$run/rust/library.sqlite" "$t0" "$t1" | tee "$run/compare.txt"

# The watcher's ascent for vanished paths.
{
    for p in "$vol" "$lib" "$A" "$A/x.mov" "$A/sub/x.mov" "$A/sub/deep/x.mov" "$A/sub/deep/deeper/x.mov" "/Volumes/Other/x.mov" "$A/../B/x.mov" "${A}X/y.mov"; do
        printf '%s\t%s|%s\n' "$p" "$A" "$B"
        printf '%s\t%s|%s\n' "$p" "$lib" "$A"
        printf '%s\t%s\n' "$p" "$vol"
    done
} >"$run/ascend.in"
"$oracle" ascend "$run/ascend.in" >"$run/ascend.swift"
"$twin" ascend "$run/ascend.in" >"$run/ascend.rust"
printf 'ascend: %s lines, %s differing\n' "$(wc -l <"$run/ascend.swift" | tr -d ' ')" "$(diff "$run/ascend.swift" "$run/ascend.rust" | grep -c '^>' || true)"

# Thumbnails: the extractor's baked picture (walk, decode, Alpha Type, checkerboard) on glance-core's DXV / HAP
# fixtures and, when given, the test media's DXV / HAP clips (read only); and the 32×32 statistics that pick the
# frame, on the first two frames of each test-media clip. Native codecs go through DXVKit Plus's AVFoundation grab
# (glance-library's AvFoundationThumbnails) on the Rust side.
th=$run/thumbs
mkdir -p "$th"
list=$run/thumb-list
: >"$list"
for f in "$core"/tests/fixtures/tiles_*.mov "$core"/tests/fixtures/tiles_*.mp4 "$core"/tests/fixtures/synthetic_*.mov; do
    cc=$(basename "$f" | sed 's/^[a-z]*_//; s/\..*//')
    case $cc in dxt1*|dxt5*|ycg6*|yg10*) echo "DXV	$f" >>"$list";; hap1*|hap5*|hapy*|hapm*|hapa*) echo "HAP	$f" >>"$list";; *) echo "NATIVE	$f" >>"$list";; esac
done
for f in "$core"/tests/fixtures/dxvkit/streams/glenc/*.mov "$core"/tests/fixtures/dxvkit/streams/synthetic/*.mov; do
    echo "DXV	$f" >>"$list"
done
if [ -n "$media" ]; then
    "$twin" fourcc "$media" | awk -F'\t' '$1 ~ /^(DXD3|DXD4|DXDI|DXV3)$/ {print "DXV\t"$2; next} $1 ~ /^Hap(1|5|Y|M|A)$/ {print "HAP\t"$2; next} {print "NATIVE\t"$2}' >>"$list"
fi
exact=0; within1=0; other=0; bothfail=0; n=0
while IFS='	' read -r kind f; do
    for a in premultiplied straight; do
        [ "$kind" != DXV ] && [ "$a" = straight ] && continue
        n=$((n + 1))
        rm -f "$th/s.rgba" "$th/r.rgba"
        "$oracle" thumb "$f" "$a" "$th/s.rgba" >"$th/s.txt"
        "$twin" thumb "$f" "$a" "$th/r.rgba" >"$th/r.txt"
        if grep -q '^error' "$th/s.txt" && grep -q '^error' "$th/r.txt"; then bothfail=$((bothfail + 1)); continue; fi
        if ! cmp -s "$th/s.txt" "$th/r.txt"; then
            other=$((other + 1))
            echo "  thumb outcome differs ($kind): swift $(cut -c1-12 "$th/s.txt"), rust $(cut -c1-12 "$th/r.txt")"
            continue
        fi
        d=$("$twin" rgbadiff "$th/s.rgba" "$th/r.rgba")
        case $d in "max 0 "*) exact=$((exact + 1));; "max 1 "*) within1=$((within1 + 1));; *) other=$((other + 1)); echo "  thumb differs ($kind $a): $d";; esac
    done
done <"$list"
echo "thumbnails: $n extractions, $exact exact, $within1 within 1, $other other, $bothfail failing on both sides"
if [ -n "$media" ]; then
    "$twin" fourcc "$media" >"$run/media-fourcc"
    : >"$run/stats"
    grep -E '^(DXD3|DXD4|DXDI|DXV3|Hap1|Hap5|HapY|HapM|HapA)	' "$run/media-fourcc" | while IFS='	' read -r cc f; do
        case $cc in DX*) lab=premultiplied;; *) lab=straight;; esac
        rm -rf "$th/fr"
        wh=$("$twin" frames "$f" "$th/fr" 2)
        w=$(echo "$wh" | cut -d' ' -f1); h=$(echo "$wh" | cut -d' ' -f2)
        for fr in "$th"/fr/*.rgba; do
            [ -f "$fr" ] || continue
            echo "$("$oracle" stats "$fr" "$w" "$h" "$lab" | cut -d' ' -f1,2) $("$twin" stats "$fr" "$w" "$h" "$lab" | cut -d' ' -f1,2) $w $h" >>"$run/stats"
        done
    done
    rm -rf "$th/fr"
    awk '{ n++; if ($1==$3 && $2==$4) e++ } END { printf "frame statistics: %d frames, %d exact\n", n, e }' "$run/stats"
    "$twin" stats-decisions "$run/stats"
fi
echo "differences, if any: diff $run/<side>/out<n>.txt; $run/compare.txt"
