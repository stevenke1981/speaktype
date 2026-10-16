#!/bin/bash
# usage: startup.sh <binary> <runs>  -> ms until the SpeakType window is mapped (Xvfb)
BIN=$1; N=${2:-5}
D=$((80 + RANDOM % 50)); Xvfb :$D -screen 0 1280x800x24 >/dev/null 2>&1 & XP=$!; sleep 1
export DISPLAY=:$D
for i in $(seq $N); do
  H=$(mktemp -d); mkdir -p $H/SpeakType/models
  ln -s /workspace/bench/ggml-large-v3-turbo.bin $H/SpeakType/models/ 2>/dev/null
  T0=$(date +%s%N)
  HOME=$H LOCALAPPDATA=$H $BIN >/dev/null 2>&1 & P=$!
  timeout 30 xdotool search --sync --onlyvisible --name "^SpeakType$" >/dev/null 2>&1 || echo "xdotool failed"
  T1=$(date +%s%N)
  echo "run=$i window_visible_ms=$(( (T1-T0)/1000000 ))"
  kill $P 2>/dev/null; wait $P 2>/dev/null; rm -rf $H
done
kill $XP
