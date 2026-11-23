#!/usr/bin/env bash
# Usage: run.sh <data dir>. Loads both installs, works them in build order, works build 1 again.
set -euo pipefail
S=$1; B="$(dirname "$0")/target/release/rannoch-shape"
rm -rf "$S"/live "$S"/live.clean "$S"/pts "$S"/graph.rannoch*
"$B" load ~/swtor/Assets "$S/live" kessel/live/1 &
"$B" load ~/swtor-pts/Assets "$S/pts" kessel/pts/2 &
wait
"$B" work "$S/live" "$S/graph.rannoch" 1 framed
du -sh "$S/graph.rannoch"
"$B" check "$S/graph.rannoch"
cp "$S/graph.rannoch" "$S/graph-build1.rannoch"
"$B" work "$S/pts" "$S/graph.rannoch" 2 framed
du -sh "$S/graph.rannoch"
"$B" check "$S/graph.rannoch"
