#!/usr/bin/env bash
# Usage: exp2.sh <data dir>, after exp.sh: the links worker on each graph, the fix's unread pass, walks.
set -uo pipefail
S=$1; B="$(cd "$(dirname "$0")" && pwd)/target/release/rannoch-shape"
quiet() { awk '!/^  /' | sed "s#$S/##g" | cut -c1-300; }
run_killing() {
  local kills=0
  while :; do
    "$@" > "$S/kill.out" 2>&1 & local p=$!
    local wait_for=$((RANDOM % 23 + 3)) t=0
    while kill -0 $p 2>/dev/null && [ $t -lt $wait_for ]; do sleep 1; t=$((t+1)); done
    if kill -0 $p 2>/dev/null; then kill -9 $p; wait $p 2>/dev/null; kills=$((kills+1)); else wait $p; local rc=$?; quiet < "$S/kill.out"; echo "kills: $kills, exit $rc"; return; fi
  done
}
( echo "=== clean links"; "$B" links "$S/g-clean.rannoch"; "$B" digest "$S/g-clean.rannoch" 1; "$B" digest "$S/g-clean.rannoch" 2; du -sh "$S/g-clean.rannoch" | quiet
  echo "=== walks"; for c in abl npc itm qst cnv hyd; do "$B" walk "$S/g-clean.rannoch" 2 $c 200; done ) > "$S/e2-clean.log" 2>&1 &
( echo "=== kill links"; run_killing "$B" links "$S/g-kill.rannoch"; "$B" digest "$S/g-kill.rannoch" 1; "$B" check "$S/g-kill.rannoch" | tail -1 ) > "$S/e2-kill.log" 2>&1 &
( echo "=== fix"; "$B" links "$S/g-fix.rannoch"
  export RUN=-resolve
  "$B" work "$S/live" "$S/g-fix.rannoch" 1 framed | quiet
  "$B" work "$S/pts" "$S/g-fix.rannoch" 2 framed | quiet
  "$B" links "$S/g-fix.rannoch"
  "$B" digest "$S/g-fix.rannoch" 1; "$B" digest "$S/g-fix.rannoch" 2; du -sh "$S/g-fix.rannoch" | quiet ) > "$S/e2-fix.log" 2>&1 &
wait
cat "$S/e2-clean.log" "$S/e2-kill.log" "$S/e2-fix.log"
