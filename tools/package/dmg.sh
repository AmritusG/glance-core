#!/usr/bin/env bash
# tools/package/dmg.sh <app.conf> — the distributable: a compressed disk image (UDZO) named
# "<APP_NAME> <VERSION>" holding the notarised app and an /Applications link, file
# <APP_NAME>-v<VERSION>.dmg in STAGE_DIR; signed, notarised and stapled itself; then its SHA-256 in
# <dmg>.sha256 (`shasum -a 256`, written after stapling, as Glance's releases do).
. "$(dirname "$0")/lib.sh"
kit_load "${1:-}"
[ -d "$APP" ] || kit_die "no app at $APP"
xcrun stapler validate "$APP" >/dev/null || kit_die "the app is not stapled: notarise it first"
here="$(dirname "$0")"
work="$(mktemp -d "$KIT_TMP/tmp.XXXXXXXX")"
mnt="$work/mnt"
mounted=0
# Leaving, on success and on failure: the image off its mount point, then no LaunchServices record
# for the staging copy or for the mounted one, then the folder. Detaching an image mounted beside
# the staging copy is what registers that copy (measured on macOS 27.0.1: not the ditto, not the
# attach; docs/handoffs/kit-dmg-record-fix.md), so the order is detach, unregister, delete.
# `lsregister -u` answers non-zero when it holds no record for a path: nothing to do then.
cleanup() {
    if [ "$mounted" = 1 ]; then
        hdiutil detach "$mnt" >/dev/null 2>&1 || hdiutil detach "$mnt" -force >/dev/null 2>&1 || true
    fi
    "$LSR" -u "$work/$APP_NAME.app" >/dev/null 2>&1 || true
    "$LSR" -u "$mnt/$APP_NAME.app" >/dev/null 2>&1 || true
    rm -r "$work"
}
trap cleanup EXIT
ditto "$APP" "$work/$APP_NAME.app"
ln -s /Applications "$work/Applications"
if [ -e "$DMG" ]; then rm "$DMG"; fi
echo "==> hdiutil create $(basename "$DMG")"
hdiutil create -volname "$APP_NAME $VERSION" -srcfolder "$work" -ov -format UDZO "$DMG" >/dev/null
# What the image holds, read back from the image itself: no build-machine path or account name in
# any file of it (the same guard as the bundle's, on the mounted contents).
mkdir "$mnt"
hdiutil attach -readonly -nobrowse -noautoopen -mountpoint "$mnt" "$DMG" >/dev/null
mounted=1
kit_path_guard "$mnt"
hdiutil detach "$mnt" >/dev/null
mounted=0
rmdir "$mnt"
"$here/sign.sh" "$1" "$DMG"
"$here/notarize.sh" "$1" "$DMG"
(cd "$(dirname "$DMG")" && shasum -a 256 "$(basename "$DMG")" > "$(basename "$DMG").sha256")
echo "==> $DMG ($(stat -f %z "$DMG") bytes), sha256 $(cut -d' ' -f1 "$DMG.sha256")"
