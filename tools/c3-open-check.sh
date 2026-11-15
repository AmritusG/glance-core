#!/usr/bin/env bash
# tools/c3-open-check.sh <app> <clip1> <clip2> <clip3> — the C3 open-file proof on an installed
# glance-sample: cold launch with a file (Finder's way: `open -a`, an Apple Event, no argv), a second
# file while running, then two files at once (the last wins). After each, the app's event log
# (~/Library/Logs/com.amritus.glancecore.sample/events.jsonl) and its on-screen windows
# (CGWindowListCopyWindowInfo, layer 0) are printed. Quits the app at the end (pkill -x).
set -euo pipefail
app="$1"; c1="$2"; c2="$3"; c3="$4"
exe="glance-sample"
log="$HOME/Library/Logs/com.amritus.glancecore.sample/events.jsonl"
windows() {
    xcrun swift - "$1" <<'SWIFT'
import CoreGraphics
let pid = Int32(CommandLine.arguments[1])!
let all = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String: Any]]
let mine = all.filter { ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }
let sizes = mine.map { w -> String in let b = w[kCGWindowBounds as String] as! [String: Any]; return "\(b["Width"]!)x\(b["Height"]!)" }
print("windows of pid \(pid): \(mine.count) \(sizes)")
SWIFT
}
if pgrep -x "$exe" >/dev/null; then echo "$exe is running: quit it first" >&2; exit 1; fi
rm -f "$log"
echo "== 1. cold launch: open -a <app> $(basename "$c1")"
open -a "$app" "$c1"; sleep 4
pid="$(pgrep -x "$exe")"; echo "pid $pid"; windows "$pid"
echo "== 2. while running: open -a <app> $(basename "$c2")"
open -a "$app" "$c2"; sleep 3
echo "pids now: $(pgrep -x "$exe" | tr '\n' ' ')"; windows "$pid"
echo "== 3. two files at once: open -a <app> $(basename "$c3") $(basename "$c1")"
open -a "$app" "$c3" "$c1"; sleep 3
echo "pids now: $(pgrep -x "$exe" | tr '\n' ' ')"; windows "$pid"
echo "== event log"
cat "$log"
pkill -x "$exe" || true; sleep 1
pgrep -x "$exe" && echo "still running" || echo "quit"
