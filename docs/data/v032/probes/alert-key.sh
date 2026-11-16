#!/usr/bin/env bash
# alert-key.sh <bin> <key-code> <out> — start the alert probe, make it frontmost, and only if its frontmost window is
# the alert (buttons "Set as Default" and "Not Now") send ONE key (53 = Esc, 36 = Return) through System Events;
# print what the probe answered. Stops (kills the probe, no key sent) on anything else.
bin="$1"; key="$2"; out="$3"
"$bin" > "$out" 2>&1 & pid=$!; sleep 3
osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $pid) to true" >/dev/null 2>&1; sleep 1
front=$(osascript -e 'tell application "System Events" to get unix id of first process whose frontmost is true' 2>/dev/null)
buttons=$(osascript -e "tell application \"System Events\" to tell (first process whose unix id is $pid) to get name of every button of every window" 2>/dev/null)
if [ "$front" != "$pid" ] || [[ "$buttons" != *"Set as Default"*"Not Now"* ]]; then
  echo "STOP: frontmost pid $front, probe $pid, buttons [$buttons]: no key sent"; kill "$pid"; exit 1
fi
osascript -e "tell application \"System Events\" to key code $key"
sleep 2
if ps -p "$pid" >/dev/null; then echo "probe still showing the alert after key $key: killed"; kill "$pid"; fi
echo "key $key -> $(cat "$out")"
