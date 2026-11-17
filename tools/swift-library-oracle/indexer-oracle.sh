#!/bin/sh
# Builds IndexerOracle (IndexerOracle.swift beside this script) against Swift MaCrate's own CrateKit / CrateCore:
# copies the MaCrate source tree (read-only input, never edited) into <work-dir>/crate with its timestamps, adds an
# executable target to the copy's Package.swift and builds it in release with -enable-testing. Prints the path of
# the built binary. Not part of the Rust workspace and not run by CI.
#
#   tools/swift-library-oracle/indexer-oracle.sh <macrate-source-dir> <work-dir>
# Run the oracle with HOME and CFFIXED_USER_HOME in a scratch folder (MaCrate's code logs and reads defaults under
# HOME); indexer-crosstest.sh does.
set -eu
[ $# -eq 2 ] || { echo "usage: $0 <macrate-source-dir> <work-dir>" >&2; exit 2; }
src=$1
work=$2
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$work"
if [ ! -d "$work/crate" ]; then
    cp -Rp "$src" "$work/crate"
    rm -rf "$work/crate/.build"
    cat >>"$work/crate/Package.swift" <<'PKG'

// Added by glance-core tools/swift-library-oracle/indexer-oracle.sh (scratch copy only).
package.targets.append(.executableTarget(name: "IndexerOracle",
                                         dependencies: ["CrateKit", "CrateCore", .product(name: "GlanceCore", package: "glance")],
                                         path: "Sources/IndexerOracle"))
PKG
fi
mkdir -p "$work/crate/Sources/IndexerOracle" "$work/home"
cp -p "$here/IndexerOracle.swift" "$work/crate/Sources/IndexerOracle/main.swift"
cd "$work/crate"
swift build -c release --product IndexerOracle -Xswiftc -enable-testing >&2
echo "$(swift build -c release --product IndexerOracle -Xswiftc -enable-testing --show-bin-path)/IndexerOracle"
