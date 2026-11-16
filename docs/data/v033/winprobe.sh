#!/bin/zsh
# winprobe.sh <bundle> <label>: open the sample with the clip, read its windows through System Events at 1..8 s, dump the Window menu, quit by pid.
SP=<scratchpad>
pid_of() { osascript -e "tell application \"System Events\" to get unix id of every process whose bundle identifier is \"com.amritus.glancecore.sample\"" 2>/dev/null | tr -d ' ' | tr ',' '\n' | head -1; }
[ -n "$(pid_of)" ] && { echo "sample already running"; exit 1; }
open -a "$1" $SP/clips/tiles_dxt5.mov; t0=$(date +%s)
for i in 1 2 3 4 5 6 8; do
  sleep 1; pid=$(pid_of)
  echo "$2 t~${i}s front=[$(osascript -e 'tell application "System Events" to get name of first process whose frontmost is true')] wins=[$(osascript -e "tell application \"System Events\" to tell (first process whose unix id is $pid) to get {name, value of attribute \"AXMain\", value of attribute \"AXFocused\", position, size} of every window" 2>&1)]"
done
xcrun swift $SP/menu-dump.swift $pid 2>&1 | grep -E '^Window \| (Minimize|Zoom|Full Screen Tile) \|'
kill $pid; sleep 1.5
