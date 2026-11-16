#!/bin/zsh
# menus.sh <sample bundle> <out dir> <label>: menu dumps of the sample bundle and the Swift reference, clip open (open -a).
SP=<scratchpad>
APP=$1; OUT=$2; L=$3; CLIP=$SP/clips/tiles_dxt5.mov; REF="$SP/swiftref/Glance SwiftRef.app"
mkdir -p $OUT
pid_of() { osascript -e "tell application \"System Events\" to get unix id of every process whose bundle identifier is \"$1\"" 2>/dev/null | tr -d ' ' | tr ',' '\n' | head -1; }
hidden() { echo "hidden apps ($1): [$(osascript -e 'tell application "System Events" to get name of every process whose visible is false and background only is false' 2>&1)]; frontmost: [$(osascript -e 'tell application "System Events" to get name of first process whose frontmost is true' 2>&1)]"; }
dump() { xcrun swift $SP/menu-dump.swift $1 2>&1 | grep -v warning; }
{
hidden "before"
[ -n "$(pid_of com.amritus.glancecore.sample)" ] && { echo "sample already running, abort"; exit 1; }
[ -n "$(pid_of com.amritus.glance.swiftref)" ] && { echo "swiftref already running, abort"; exit 1; }
open -a "$APP" $CLIP; sleep 4; pid=$(pid_of com.amritus.glancecore.sample); echo "sample pid $pid"
hidden "sample up"
dump $pid > $OUT/menus-$L-sample.txt; kill $pid; sleep 1.5
defaults write com.amritus.glance.swiftref Glance.defaultHandlerPromptShown -bool true
open -a "$REF" $CLIP; sleep 4; pid=$(pid_of com.amritus.glance.swiftref); echo "swiftref pid $pid"
hidden "swiftref up"
dump $pid > $OUT/menus-$L-swift.txt; kill $pid; sleep 1.5
hidden "after"
sed 's/Glance Sample/Glance/g' $OUT/menus-$L-sample.txt | diff $OUT/menus-$L-swift.txt - > $OUT/menus-$L-diff.txt; echo "diff (swift <, sample with \"Glance Sample\" read as \"Glance\" >), exit $?:"; cat $OUT/menus-$L-diff.txt
} 2>&1 | tee $OUT/menus-$L-run.txt
