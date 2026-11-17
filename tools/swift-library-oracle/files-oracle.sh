#!/bin/sh
# Builds FilesOracle (FilesOracle.swift beside this script) against Swift MaCrate's own CrateKit /
# CrateCore: copies the MaCrate source tree (read-only input, never edited) into <work-dir>/crate with
# its timestamps, adds an executable target to the copy's Package.swift and builds it in release with
# -enable-testing (the oracle calls three internal functions through @testable import). Prints the
# path of the built binary. Not part of the Rust workspace and not run by CI.
#
#   tools/swift-library-oracle/files-oracle.sh <macrate-source-dir> <work-dir>
#   HOME=<scratch> CFFIXED_USER_HOME=<scratch> "$(…/files-oracle.sh …)" files <dir>   # modes: FilesOracle.swift
# Run the oracle with HOME and CFFIXED_USER_HOME in a scratch folder: MaCrate's code logs to
# ~/Library/Logs/com.amritus.crate/diag.log and reads UserDefaults / Application Support under HOME.
set -eu
[ $# -eq 2 ] || { echo "usage: $0 <macrate-source-dir> <work-dir>" >&2; exit 2; }
src=$1
work=$2
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$work"
if [ ! -d "$work/crate" ]; then
    cp -Rp "$src" "$work/crate"
    rm -rf "$work/crate/.build"
    cat >>"$work/crate/Package.swift" <<'EOF'

// Added by glance-core tools/swift-library-oracle/files-oracle.sh (scratch copy only).
package.targets.append(.executableTarget(name: "FilesOracle", dependencies: ["CrateKit", "CrateCore"],
                                         path: "Sources/FilesOracle"))
EOF
fi
mkdir -p "$work/crate/Sources/FilesOracle"
cp -p "$here/FilesOracle.swift" "$work/crate/Sources/FilesOracle/main.swift"
cd "$work/crate"
swift build -c release --product FilesOracle -Xswiftc -enable-testing >&2
echo "$(swift build -c release --product FilesOracle -Xswiftc -enable-testing --show-bin-path)/FilesOracle"
