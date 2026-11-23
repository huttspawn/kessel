# Kessel

SWTOR client miner. The repository restarted empty (#432); see README.md.

## DO NOT PUT SHIT IN HERE. RECALL/REFLECT.

This file is for **basic operational info only**. Everything else lives in legion:

- `legion recall --repo kessel --context "..."` — your own prior findings
- `legion consult --context "..."` — every agent's findings
- `legion reflect --repo kessel --text "..."` — write a new finding

Findings, format notes, decode results and design prose do not belong in this file.

## Where things are

- This tree: the rebuild. Empty until the design asks for its first crate.
- `oracle` branch: the previous tree, whole (main at 2619fa1). A reference for how something was done and for data to compare against; never an acceptance gate.
- `salvage/*`, `rev/scaffold` and the `research/*` branches: uncommitted work saved from old worktrees.

## Build and test

CI runs `cargo check`, `test`, `clippy -D warnings` and `fmt --check` across the workspace once a `Cargo.toml` exists, and skips cargo until then.
