#!/usr/bin/env bash
# tools/swift-ref.sh <dest-dir> — build Swift Glance from the parity reference (.ref/glance @ a9dc601, 0.18.1)
# as "<dest-dir>/Glance SwiftRef.app" under the throw-away id com.amritus.glance.swiftref, ad-hoc signed, so it
# can sit beside the test build without touching Amrit's Glance (com.amritus.glance, /Applications/Glance.app).
# The source is copied out of .ref first (read-only clone, nothing built inside it). Prints the app path.
# Remove its LaunchServices record after use: lsregister -u "<dest-dir>/Glance SwiftRef.app".
set -euo pipefail
dest="$1"
root=<workspace>/glance-core
ref="$root/.ref/glance"
app="$dest/Glance SwiftRef.app"
[ "$(git -C "$ref" rev-parse --short HEAD)" = a9dc601 ] || { echo "$ref is not at a9dc601" >&2; exit 1; }
mkdir -p "$dest"
rsync -a --delete --exclude .build --exclude .git "$ref/" "$dest/src/"
(cd "$dest/src" && swift build -c release >&2)
bin="$dest/src/.build/release/Glance"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Frameworks"
cp -p "$bin" "$app/Contents/MacOS/Glance"
cp -p "$dest/src/Sources/Glance/Info.plist" "$app/Contents/Info.plist"
cp -p "$dest/src/assets/Glance.icns" "$app/Contents/Resources/AppIcon.icns"
find "$dest/src/.build/release" -maxdepth 2 -name '*.framework' -type d -exec cp -Rp {} "$app/Contents/Frameworks/" \;
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier com.amritus.glance.swiftref" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleName Glance" "$app/Contents/Info.plist"
codesign --force --deep --sign - "$app" >&2
echo "$app"
