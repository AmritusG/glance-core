#!/usr/bin/env bash
# tools/package/bundle.sh <app.conf> — build the app universal (arm64 + x86_64, macOS MIN_MACOS)
# and assemble <STAGE_DIR>/<APP_NAME>.app: the lipo'd binary, Info.plist from the template with the
# slots filled, the icon when ICON is set. Refuses when the plist's two version fields differ (Swift
# Glance's build.sh rule, v0.18.1: LaunchServices compares CFBundleVersion).
#
# The Rust toolchain must have the std of both macOS targets; KIT_TOOLCHAIN_BIN points at one
# (default: the toolchain on PATH when it has both, else rustup's stable).
. "$(dirname "$0")/lib.sh"
kit_load "${1:-}"

bin_dir="${KIT_TOOLCHAIN_BIN:-}"
if [ -z "$bin_dir" ]; then
    if [ -d "$(rustc --print sysroot)/lib/rustlib/x86_64-apple-darwin" ] && [ -d "$(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin" ]; then
        bin_dir="$(dirname "$(command -v cargo)")"
    else
        bin_dir="$HOME/.rustup/toolchains/stable-$(uname -m | sed 's/arm64/aarch64/')-apple-darwin/bin"
    fi
fi
[ -x "$bin_dir/cargo" ] && [ -x "$bin_dir/rustc" ] || kit_die "no cargo / rustc in $bin_dir (set KIT_TOOLCHAIN_BIN)"
echo "==> toolchain: $("$bin_dir/rustc" --version)"

target_dir="$STAGE_DIR/target"
mkdir -p "$STAGE_DIR"
# No build-machine paths in the shipped binary (Swift MaCrate 0.16.3's check-build-paths.sh rule): the source paths
# rustc writes for panic locations name the cargo home (registry, git checkouts), the repo and the stage's target
# (generated code), so each is remapped to a neutral prefix. RUSTFLAGS takes the place of every rustflags in
# .cargo/config.toml (build and target.<triple>; Cargo uses one source), so a config's `-Wl,-S` does not apply here:
# `strip -S` below does that work. A build that still holds $HOME is refused below.
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
# With the toolchain's rust-src component installed, rustc writes the local copy's path for std code inlined into the
# app instead of std's own /rustc/<hash> (GlancEhr 1.0.4: rust-src arrived on the build machine and 92 such paths
# reached the binary): that folder is remapped too.
rust_src="$("$bin_dir/rustc" --print sysroot)/lib/rustlib/src/rust"
remap="--remap-path-prefix=$cargo_home=/cargo --remap-path-prefix=$REPO=/src --remap-path-prefix=$target_dir=/target --remap-path-prefix=$rust_src=/rustc/src"
# The speed build (Amrit 2026-11-18: "Any software under 200 MB I want optimized for speed, not disk size"): whole-
# program optimisation by default for every app the kit builds, whatever its Cargo.toml says; an app that measured
# slower sets the CARGO_PROFILE_RELEASE_* variables itself. panic and debug / strip stay the app's profile's.
export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-fat}"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-1}"
export CARGO_PROFILE_RELEASE_OPT_LEVEL="${CARGO_PROFILE_RELEASE_OPT_LEVEL:-3}"
echo "==> release profile: lto $CARGO_PROFILE_RELEASE_LTO, codegen-units $CARGO_PROFILE_RELEASE_CODEGEN_UNITS, opt-level $CARGO_PROFILE_RELEASE_OPT_LEVEL"
# The Intel slice for x86-64-v3 (AVX2, FMA, BMI1 / 2, F16C, LZCNT, MOVBE): every Intel Mac macOS 14 runs on has them
# (Apple's "macOS Sonoma is compatible with these computers", support.apple.com/105113: iMac Pro 2017 and later — the
# oldest cores are Skylake-SP and Amber Lake, both Haswell-or-later). arm64 needs nothing: aarch64-apple-darwin
# already targets apple-m1.
cpu_flags() { case "$1" in x86_64-*) echo "-C target-cpu=x86-64-v3";; *) echo "";; esac; }
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    flags="${RUSTFLAGS:+$RUSTFLAGS }$(cpu_flags "$t")"
    echo "==> cargo build --release --target $t -p $CARGO_PACKAGE ${flags:+(RUSTFLAGS $flags + path remaps)}"
    (cd "$REPO" && MACOSX_DEPLOYMENT_TARGET="$MIN_MACOS" CARGO_TARGET_DIR="$target_dir" RUSTC="$bin_dir/rustc" RUSTFLAGS="${flags:+$flags }$remap" "$bin_dir/cargo" build --release --locked --target "$t" -p "$CARGO_PACKAGE" --bin "$EXECUTABLE")
done

echo "==> assembling $APP"
case "$APP" in */"$APP_NAME".app) ;; *) kit_die "odd bundle path $APP";; esac
if [ -e "$APP" ]; then rm -r "$APP"; fi
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/$EXECUTABLE" "$target_dir/aarch64-apple-darwin/release/$EXECUTABLE" "$target_dir/x86_64-apple-darwin/release/$EXECUTABLE"
# Debug information out of the shipped binary, the symbol table kept (crash reports keep function names): Apple's
# `strip -S` on the universal file. Cargo's `strip = "debuginfo"` runs rustup's rust-objcopy, which on rustc 1.100
# cannot load (it looks for libLLVM.dylib beside itself, not in the toolchain's lib/) and only warns (v0.4.0).
strip -S "$APP/Contents/MacOS/$EXECUTABLE"
stabs="$(nm -a "$APP/Contents/MacOS/$EXECUTABLE" 2>/dev/null | grep -c ' - ' || true)"
[ "$stabs" = 0 ] || kit_die "debug map entries left after strip -S: $stabs"
home_paths="$(strings -a "$APP/Contents/MacOS/$EXECUTABLE" | grep -cF "$HOME" || true)"
[ "$home_paths" = 0 ] || kit_die "build-machine paths ($HOME) left in the binary: $home_paths"
sed -e "s|@APP_NAME@|$APP_NAME|g" -e "s|@BUNDLE_ID@|$BUNDLE_ID|g" -e "s|@EXECUTABLE@|$EXECUTABLE|g" -e "s|@VERSION@|$VERSION|g" -e "s|@MIN_MACOS@|$MIN_MACOS|g" \
    "$CONF_DIR/$INFO_PLIST" > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" >/dev/null
short="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")"
long="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$APP/Contents/Info.plist")"
[ "$short" = "$long" ] || kit_die "CFBundleVersion ($long) != CFBundleShortVersionString ($short): set both to the release version"
[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Contents/Info.plist")" = "$BUNDLE_ID" ] || kit_die "Info.plist bundle id is not $BUNDLE_ID"
if [ -n "${ICON:-}" ]; then
    cp -p "$CONF_DIR/$ICON" "$APP/Contents/Resources/AppIcon.icns"
fi
kit_path_guard "$APP"
echo "==> stripped: 0 debug map entries, 0 build-machine paths, $(nm "$APP/Contents/MacOS/$EXECUTABLE" 2>/dev/null | wc -l | tr -d ' ') symbols kept"
echo "==> $APP: version $short, $(lipo -archs "$APP/Contents/MacOS/$EXECUTABLE"), minos $(otool -l "$APP/Contents/MacOS/$EXECUTABLE" | awk '/LC_BUILD_VERSION/{f=1} f&&/minos/{print $2; exit}')"
