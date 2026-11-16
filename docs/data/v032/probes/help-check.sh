#!/usr/bin/env bash
# help-check.sh <pid> <app-title> — press Help ▸ "<app-title> Help" through Accessibility in that process only
# (no mouse, no keys), wait, print every window title / text / button that process and the Help Viewer show,
# then close what appeared: the process's alert by its OK button, Help Viewer by quitting it. Read-out only else.
pid="$1"; name="$2"
osascript -e "tell application \"System Events\" to tell (first process whose unix id is $pid) to click menu item \"$name Help\" of menu 1 of menu bar item \"Help\" of menu bar 1" >/dev/null 2>&1
sleep 3
echo "-- windows of pid $pid:"
osascript -e "tell application \"System Events\" to tell (first process whose unix id is $pid) to get {name of every window, value of every static text of every window, name of every button of every window}" 2>&1
echo "-- Help Viewer / Tips running:"; pgrep -lx "HelpViewer" ; pgrep -lf "Tips.app" | head -2
osascript -e "tell application \"System Events\" to tell (first process whose unix id is $pid) to if exists (first window whose (exists button \"OK\")) then click button \"OK\" of (first window whose (exists button \"OK\"))" >/dev/null 2>&1
