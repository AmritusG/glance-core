#!/usr/bin/env bash
# safe-drop.sh <bin> <clips> <tries> <out> — glance-rs's g2-drop-crash.sh sample side (DRAG_LINGER=0), made safe:
# the drag is tools/safe-drag (drops only on the app under test, else aborts); the run STOPS at the first abort, at
# the first "drag did not end", and after the first crash unless CONTINUE_AFTER_CRASH=1; after a crash it waits for
# macOS's "<app> quit unexpectedly." dialog, presses its Ignore (tools/dismiss-crash.sh), and stops if it is still there. Prints one line per try.
set -uo pipefail
bin="$1"; clips="$2"; tries="$3"; out="$4"
here="$(cd "$(dirname "$0")" && pwd)"
geo() { xcrun swift "$here/window-id.swift" "$1" 2>/dev/null; }
title_of() { osascript -e "tell application \"System Events\" to tell (first process whose unix id is $1) to return name of window 1" 2>/dev/null; }
reports() { find ~/Library/Logs/DiagnosticReports -maxdepth 1 -name "$(basename "$bin")-*" | wc -l | tr -d ' '; }
: > "$out"
for try in $(seq 1 "$tries"); do
  before=$(reports)
  "$bin" "$clips/tiles_dxt1.mov" >/dev/null 2>&1 & pid=$!; sleep 3
  read -r _ x y w h < <(geo "$pid") || { echo "try $try: STOP: no window for pid $pid" | tee -a "$out"; kill "$pid" 2>/dev/null; exit 1; }
  res=$(TARGET_PID=$pid DRAG_LINGER=0 "$here/safe-drag" $((x + w / 2)) $((y + 200)) "$clips/tiles_hap1.mov" 2>&1); code=$?
  sleep 1; t=$(title_of "$pid"); sleep 3
  after=$(reports)
  if ps -p "$pid" >/dev/null; then state="alive"; kill "$pid"; sleep 1.5; else state="GONE"; fi
  if [ $state = GONE ]; then
    # macOS shows "<app> quit unexpectedly." (the DialogType default does not stop it on macOS 27): wait for it,
    # press its Ignore, and stop the run if one is still there.
    for _ in 1 2 3 4 5 6; do sleep 1; osascript -e "tell application \"System Events\" to tell (every process whose name is \"UserNotificationCenter\") to count (every window whose value of static texts contains \"$(basename "$bin") quit unexpectedly.\")" 2>/dev/null | grep -q '[1-9]' && break; done
    "$here/dismiss-crash.sh" "$(basename "$bin")" >/dev/null || { echo "STOP: a crash dialog could not be dismissed" | tee -a "$out"; exit 1; }
  fi
  echo "try $try: $res; title after drop [$t]; app $state; new crash reports: $((after - before))" | tee -a "$out"
  if [ $code -ne 0 ] || [[ "$res" == *"did not end"* ]]; then echo "STOP: the drag tool did not drop cleanly (exit $code)" | tee -a "$out"; exit 1; fi
  if [ $state = GONE ] && [ "${CONTINUE_AFTER_CRASH:-0}" != 1 ]; then echo "STOP: crashed; stopping before a crash dialog can sit under the next drop" | tee -a "$out"; exit 2; fi
done
