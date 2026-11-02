#!/bin/sh
# Builds rannoch v0.5.0 with known-at.patch beside this toy, then runs the toy.
# RANNOCH is the rannoch checkout to take the v0.5.0 tag from.
set -e
cd "$(dirname "$0")"
: "${RANNOCH:=/Volumes/store/projects/runlegion/rannoch}"
if [ ! -d rannoch-patched ]; then
  mkdir rannoch-patched
  git -C "$RANNOCH" archive v0.5.0 crates Cargo.toml Cargo.lock rust-toolchain.toml | tar -x -C rannoch-patched
  patch rannoch-patched/crates/rannoch-store/src/history.rs known-at.patch
fi
cargo run --quiet
