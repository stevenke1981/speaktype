#!/bin/bash
# usage: linux-screenshots.sh <binary> <outdir>   (needs Xvfb + ImageMagick `import`)
set -u
BIN=$1; OUT=$2; mkdir -p "$OUT"
D=$((140 + RANDOM % 50)); Xvfb :$D -screen 0 1100x800x24 >/dev/null 2>&1 & XP=$!; sleep 1
export DISPLAY=:$D
shot() { # name, env...
  local name=$1; shift
  H=$(mktemp -d)
  env "$@" SPEAKTYPE_SKIP_MODEL_LOAD=1 HOME=$H LOCALAPPDATA=$H "$BIN" >/dev/null 2>&1 & P=$!
  sleep 4
  import -window root "$OUT/$name.png"
  kill $P 2>/dev/null; wait $P 2>/dev/null; rm -rf "$H"
  echo "saved $OUT/$name.png"
}
shot linux-main SPEAKTYPE_X=1
shot linux-settings SPEAKTYPE_UI_PAGE=settings
shot linux-settings-speed SPEAKTYPE_UI_PAGE=speed
shot linux-indicator SPEAKTYPE_DEMO_INDICATOR=1
kill $XP
