#!/usr/bin/env bash
# Usage: exp4.sh <data dir>. Nodes without kills, then links under kill -9, on a new graph.
set -uo pipefail
S=$1; B="$(cd "$(dirname "$0")" && pwd)/target/release/rannoch-shape"; G="$S/g-k4.rannoch"; O="$S/k4.out"
rm -f "$G"* "$S"/g-k3.rannoch*
quiet() { awk '!/^  /' | sed "s#$S/##g" | cut -c1-300; }
run_killing() {
  local kills=0
  while :; do
    "$@" > "$O" 2>&1 & local p=$!
    local wait_for=$((RANDOM % 18 + 8)) t=0
    while kill -0 $p 2>/dev/null && [ $t -lt $wait_for ]; do sleep 1; t=$((t+1)); done
    if kill -0 $p 2>/dev/null; then kill -9 $p; wait $p 2>/dev/null; kills=$((kills+1)); else wait $p; local rc=$?; quiet < "$O"; echo "kills: $kills, exit $rc"; return; fi
  done
}
export RUN=-k4
"$B" work "$S/live" "$G" 1 framed | quiet
run_killing "$B" links "$G"
"$B" digest "$G" 1
sqlite3 -readonly "$G" "select substr(json_extract(body,'\$.type'),1,12), count(*), count(distinct json_extract(body,'\$.id')) from _events group by 1"
