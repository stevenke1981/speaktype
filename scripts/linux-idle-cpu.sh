#!/usr/bin/env bash
# Usage: linux-idle-cpu.sh <binary> [seconds]  -> prints idle CPU % of the GUI (window visible, no model).
set -euo pipefail
BIN=$1; SECS=${2:-30}
D=:$((RANDOM % 400 + 300))
Xvfb $D -screen 0 1280x900x24 >/dev/null 2>&1 & XP=$!
sleep 1
H=$(mktemp -d)
DISPLAY=$D SPEAKTYPE_HOME=$H SPEAKTYPE_SKIP_MODEL_LOAD=1 LIBGL_ALWAYS_SOFTWARE=1 "$BIN" >/dev/null 2>&1 & AP=$!
sleep 6
tick=$(getconf CLK_TCK)
t0=$(awk '{print $14+$15}' /proc/$AP/stat); s0=$(date +%s.%N)
sleep "$SECS"
t1=$(awk '{print $14+$15}' /proc/$AP/stat); s1=$(date +%s.%N)
rss=$(awk '/VmRSS/{print $2}' /proc/$AP/status)
kill $AP 2>/dev/null || true; wait $AP 2>/dev/null || true
kill $XP 2>/dev/null || true
python3 -c "print(f'idle CPU: {($t1-$t0)/$tick/($s1-$s0)*100:.2f}% over {$s1-$s0:.0f}s, RSS {$rss/1024:.0f} MB')"
