#!/bin/bash
# measure.sh <label> <out-dir> <clip>: glance-proof plays <clip> for 30 s (no audio, frame log on); from 10 s to 20 s
# after its window appears another process's window covers it fully (cover); CPU time is sampled every 0.5 s.
set -u
label=$1 out=$2 clip=$3 here=$(cd "$(dirname "$0")" && pwd)
proof=${PROOF:-target/release/glance-proof}
if pgrep -x glance-proof >/dev/null; then echo "glance-proof already running"; exit 1; fi
mkdir -p "$out"
"$proof" --seconds 30 --warmup 0 --no-audio --log "$out/$label-frames.csv" --out "$out/$label-report.json" "$clip" >/dev/null 2>"$out/$label-proof.err" &
pid=$!
"$here/cover" "$pid" 10 10 >"$out/$label-cover.txt" &
now() { python3 -c 'import time; print(int(time.time() * 1000))'; }
while kill -0 "$pid" 2>/dev/null; do echo "$(now) $(ps -o time= -p "$pid")"; sleep 0.5; done >"$out/$label-cpu.txt"
wait
pkill -9 -x glance-proof 2>/dev/null
exit 0
