#!/usr/bin/env bash
# drop-env.sh <bin> <clips> <tries> <logdir> — g2-drop-crash.sh's sample side, with the sample's stderr kept per try
# and extra environment passed through (e.g. NSZombieEnabled=YES). DRAG_LINGER=0.
set -uo pipefail
bin="$1"; clips="$2"; tries="$3"; logs="$4"; mkdir -p "$logs"
here="$(cd "$(dirname "$0")" && pwd)"
title_of() { osascript -e "tell application \"System Events\" to tell (first process whose unix id is $1) to return name of window 1" 2>/dev/null; }
geo() { xcrun swift "$here/window-id.swift" "$1" 2>/dev/null; }
reports() { find ~/Library/Logs/DiagnosticReports -maxdepth 1 -name 'glance-sample-*' | wc -l | tr -d ' '; }
for try in $(seq 1 "$tries"); do
  before=$(reports)
  "$bin" "$clips/tiles_dxt1.mov" >"$logs/try$try.log" 2>&1 & pid=$!; sleep 3
  read -r _ x y w h < <(geo "$pid")
  res=$(DRAG_LINGER=0 xcrun swift "$here/drag-file.swift" $((x + w / 2)) $((y + 200)) "$clips/tiles_hap1.mov" 2>&1 | grep -v warning)
  sleep 1; t=$(title_of "$pid"); sleep 3
  after=$(reports)
  if ps -p "$pid" >/dev/null; then state="alive"; kill "$pid"; sleep 1.5; else state="GONE"; fi
  echo "try $try: $res; title after drop [$t]; app $state; new crash reports: $((after - before))"
done 2>/dev/null
