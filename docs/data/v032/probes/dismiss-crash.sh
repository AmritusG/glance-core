#!/usr/bin/env bash
# dismiss-crash.sh <app-name> — press "Ignore" on macOS's "<app-name> quit unexpectedly." dialogs, and only those
# (UserNotificationCenter windows whose text names that app). Prints how many it dismissed; exit 1 if one is left.
name="$1"
osascript - "$name" <<'OSA'
on run argv
  set target to (item 1 of argv) & " quit unexpectedly."
  set n to 0
  tell application "System Events"
    repeat with p in (every process whose name is "UserNotificationCenter")
      repeat with w in (every window of p)
        set texts to value of every static text of w
        if texts contains target then
          click button "Ignore" of w
          set n to n + 1
        end if
      end repeat
    end repeat
  end tell
  return n
end run
OSA
sleep 1
left=$(osascript -e "tell application \"System Events\" to tell (every process whose name is \"UserNotificationCenter\") to count (every window whose value of static texts contains \"$name quit unexpectedly.\")" 2>/dev/null | tr -dc '0-9')
[ "${left:-0}" = 0 ]
