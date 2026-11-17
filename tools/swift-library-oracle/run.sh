#!/bin/zsh
# Runs glance-library's Swift oracle against a scratch copy of Swift MaCrate's source.
#
#   run.sh goldens <macrate-src> <work-dir> <out-dir>
#       Swift writes the golden fixtures (libraries, dumps, reads, JSON bytes, rules) and the Unicode tables.
#   run.sh verify <macrate-src> <work-dir> <rust-dir> <golden-dir> <report-dir>
#       Swift opens, reads and writes what `cargo run -p glance-library --example write-fixtures -- <rust-dir>` made.
#
# <macrate-src> is a checkout of Swift MaCrate (0.16.4); it is copied, never changed. The test process runs with
# HOME and CFFIXED_USER_HOME set to <work-dir>/home, so nothing it logs or stores lands in the real user's Library.
set -euo pipefail

here=${0:A:h}
mode=${1:?mode: goldens | verify}
src=${2:?MaCrate source dir}
work=${3:?work dir}
mkdir -p "$work"
work=${work:A}

if [[ ! -d "$work/crate" ]]; then
    cp -Rp "$src" "$work/crate"
fi
mkdir -p "$work/crate/Tests/LibraryOracleTests" "$work/home"
cp -p "$here/LibraryOracleTests.swift" "$work/crate/Tests/LibraryOracleTests/"
if ! grep -q LibraryOracleTests "$work/crate/Package.swift"; then
    cat >> "$work/crate/Package.swift" <<'EOF'

package.targets.append(.testTarget(
    name: "LibraryOracleTests",
    dependencies: ["CrateKit", .product(name: "GRDB", package: "GRDB.swift"), .product(name: "GlanceCore", package: "glance")],
    path: "Tests/LibraryOracleTests"
))
EOF
fi

(cd "$work/crate" && swift build --build-tests 2>&1 | tail -3)
# SwiftPM's new build system makes one bundle per test target; the old one, one per package.
bundle="$work/crate/.build/debug/LibraryOracleTests.xctest"
[[ -d "$bundle" ]] || bundle=$(ls -d "$work"/crate/.build/debug/*PackageTests.xctest | head -1)

run_test() {
    env HOME="$work/home" CFFIXED_USER_HOME="$work/home" "$@" xcrun xctest -XCTest "LibraryOracleTests.LibraryOracleTests/$test_name" "$bundle"
}

case $mode in
goldens)
    out=${4:?out dir}
    mkdir -p "$out"
    out=${out:A}
    rm -f "$work/legacy-v7.sqlite"
    sqlite3 "$work/legacy-v7.sqlite" < "$here/legacy-v7.sql"
    test_name=testWriteGoldens run_test ORACLE_OUT="$out" ORACLE_LEGACY="$work/legacy-v7.sqlite"
    test_name=testWriteUnicodeTables run_test ORACLE_OUT="$out"
    ;;
verify)
    rust=${4:?rust dir}; golden=${5:?golden dir}; report=${6:?report dir}
    mkdir -p "$report"
    test_name=testReadRustWritten run_test ORACLE_RUST="${rust:A}" ORACLE_GOLDEN="${golden:A}" ORACLE_REPORT="${report:A}"
    ;;
*)
    echo "unknown mode $mode" >&2
    exit 2
    ;;
esac
