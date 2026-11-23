#!/usr/bin/env bash
# Usage: exp.sh <data dir holding live/ and pts/>. Clean, repeat, kill and fix runs; each on its own graph.
set -uo pipefail
S=$1; B="$(cd "$(dirname "$0")" && pwd)/target/release/rannoch-shape"
quiet() { awk '!/^  /' | sed "s#$S/##g" | cut -c1-300; }
rm -f "$S"/g-*.rannoch*

echo "=== clean"
export RUN=-clean
"$B" work "$S/live" "$S/g-clean.rannoch" 1 framed | quiet
"$B" links "$S/g-clean.rannoch"
"$B" digest "$S/g-clean.rannoch" 1
"$B" work "$S/pts" "$S/g-clean.rannoch" 2 framed | quiet
"$B" links "$S/g-clean.rannoch"
"$B" digest "$S/g-clean.rannoch" 1
"$B" digest "$S/g-clean.rannoch" 2
du -sh "$S/g-clean.rannoch"

echo "=== repeat: same bytes, same reader code under a new name, must write nothing"
export RUN=-repeat
"$B" work "$S/pts" "$S/g-clean.rannoch" 2 framed2 | quiet
"$B" links "$S/g-clean.rannoch"
"$B" digest "$S/g-clean.rannoch" 2

echo "=== walks"
for c in abl npc itm qst cnv hyd; do "$B" walk "$S/g-clean.rannoch" 2 $c 200; done

run_killing() { # kills the command at random, 3-25 s in, until it exits by itself
  local kills=0
  while :; do
    "$@" > "$S/kill.out" 2>&1 & local p=$!
    local wait_for=$((RANDOM % 23 + 3)) t=0
    while kill -0 $p 2>/dev/null && [ $t -lt $wait_for ]; do read -t 1 < <(:) ; sleep 1; t=$((t+1)); done
    if kill -0 $p 2>/dev/null; then kill -9 $p; wait $p 2>/dev/null; kills=$((kills+1)); else wait $p; local rc=$?; cat "$S/kill.out" | quiet; echo "kills: $kills, exit $rc"; return; fi
  done
}
( echo "=== kill"
  export RUN=-kill
  run_killing "$B" work "$S/live" "$S/g-kill.rannoch" 1 framed
  run_killing "$B" links "$S/g-kill.rannoch"
  "$B" digest "$S/g-kill.rannoch" 1
  "$B" check "$S/g-kill.rannoch" | tail -1 ) > "$S/exp-kill.log" 2>&1 &
( echo "=== compare readers"
  "$B" compare "$S/live"
  echo "=== fix: old reader first, then the framed reader over the same builds"
  export RUN=-fix
  "$B" work "$S/live" "$S/g-fix.rannoch" 1 old | quiet
  "$B" links "$S/g-fix.rannoch"
  "$B" work "$S/pts" "$S/g-fix.rannoch" 2 old | quiet
  "$B" links "$S/g-fix.rannoch"
  "$B" digest "$S/g-fix.rannoch" 1
  "$B" digest "$S/g-fix.rannoch" 2
  du -sh "$S/g-fix.rannoch"
  date +%s%3N > "$S/fix-mark"
  echo "mark $(cat "$S/fix-mark")"
  "$B" work "$S/live" "$S/g-fix.rannoch" 1 framed | quiet
  "$B" links "$S/g-fix.rannoch"
  "$B" digest "$S/g-fix.rannoch" 2
  "$B" work "$S/pts" "$S/g-fix.rannoch" 2 framed | quiet
  "$B" links "$S/g-fix.rannoch"
  "$B" digest "$S/g-fix.rannoch" 1
  "$B" digest "$S/g-fix.rannoch" 2
  du -sh "$S/g-fix.rannoch" ) > "$S/exp-fix.log" 2>&1 &
wait
cat "$S/exp-kill.log" "$S/exp-fix.log"
