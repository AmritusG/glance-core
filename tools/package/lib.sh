# tools/package/lib.sh — shared by the kit's scripts: reads the app's conf, checks the slots,
# finds the signing identity in the keychain, masks identities in what it prints.
# Sourced, never run. Every script takes the conf file as its first argument.
set -euo pipefail

kit_die() { echo "kit: $*" >&2; exit 1; }

kit_load() {
    local conf="${1:-}"
    [ -n "$conf" ] && [ -f "$conf" ] || kit_die "usage: $(basename "$0") <app.conf> ...  (conf not found: ${conf:-none})"
    CONF_DIR="$(cd "$(dirname "$conf")" && pwd)"
    # shellcheck disable=SC1090
    . "$conf"
    for slot in APP_NAME BUNDLE_ID EXECUTABLE CARGO_PACKAGE REPO INFO_PLIST VERSION MIN_MACOS NOTARY_PROFILE INSTALL_DIR STAGE_DIR; do
        [ -n "${!slot:-}" ] || kit_die "slot $slot is empty in $conf"
    done
    case "$BUNDLE_ID" in *[!A-Za-z0-9.-]*) kit_die "BUNDLE_ID '$BUNDLE_ID' has characters a bundle id cannot";; esac
    # Never stage inside or beside the project tree (working rule 8).
    case "$STAGE_DIR/" in "$REPO"/*) kit_die "STAGE_DIR is inside the project ($STAGE_DIR)";; esac
    [ "$(dirname "$STAGE_DIR")" != "$(dirname "$REPO")" ] || kit_die "STAGE_DIR is beside the project ($STAGE_DIR)"
    # STAGE_DIR as its physical path: LaunchServices records a bundle by its resolved path
    # ($TMPDIR is /var/folders/..., /var a link to /private/var), and ls-cleanup.sh compares
    # records with STAGE_DIR, so a path through a symlink would leave the staged copy registered.
    mkdir -p "$STAGE_DIR" || kit_die "cannot create STAGE_DIR ($STAGE_DIR)"
    STAGE_DIR="$(cd "$STAGE_DIR" && pwd -P)"
    APP="$STAGE_DIR/$APP_NAME.app"
    KEEP="$INSTALL_DIR/$APP_NAME.app"
    DMG="$STAGE_DIR/${DMG_NAME:-$APP_NAME-v$VERSION.dmg}"
    ENTITLEMENTS="${ENTITLEMENTS:-$(dirname "${BASH_SOURCE[0]}")/entitlements.plist}"
    LSR=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
}

# No build-machine path or account name in what ships: the family's path guard (tools/path-guard.sh,
# beside the kit in every repo that has the kit) over files and folders, binaries included.
kit_path_guard() {
    local guard
    guard="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/path-guard.sh"
    [ -x "$guard" ] || kit_die "no tools/path-guard.sh beside the kit (the kit is copied with it)"
    "$guard" files "$@" || kit_die "build-machine paths or names in what ships (above); fix them at the source"
}

# The signing identity: SIGN_IDENTITY from the conf, or "auto" = the first "Developer ID
# Application" identity in the keychain, by its hash (nothing identifying lives in the repo).
kit_identity() {
    if [ "${SIGN_IDENTITY:-auto}" != "auto" ]; then echo "$SIGN_IDENTITY"; return; fi
    local id
    id="$(security find-identity -v -p codesigning | awk '/Developer ID Application/ { print $2; exit }')"
    [ -n "$id" ] || kit_die "no Developer ID Application identity in the keychain"
    echo "$id"
}

# Print a command's output with identity names, team ids and certificate hashes masked.
kit_mask() { sed -E 's/[0-9A-F]{40}/<identity>/g; s/(Developer ID [A-Za-z]+: )[^(]*\(([A-Z0-9]{10})\)/\1<name> (<team>)/g; s/TeamIdentifier=[A-Z0-9]{10}/TeamIdentifier=<team>/g'; }
