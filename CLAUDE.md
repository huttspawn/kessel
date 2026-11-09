# Kessel

SWTOR game data extractor. Parses binary `.tor` archives, outputs SQLite.

## DO NOT PUT SHIT IN HERE. RECALL/REFLECT.

This file is for **basic operational info only**. Everything else lives in legion:

- `legion recall --repo kessel --context "..."` — your own prior findings
- `legion consult --context "..."` — every agent's findings
- `legion reflect --repo kessel --text "..."` — write a new finding

Findings (format bytes, decode breakthroughs, "known gaps", architecture
prose, per-system notes) DO NOT belong in this file. They rot here while
legion stays current. Last time this file lied to me about per-property
byte decode being unsolved when reflection `019e4d75` had solved it five
days earlier. Don't repeat that.

## Workspace layout

- `kessel/` — production crate (the `kessel` binary + library). What ships.
- `kessel-discovery/` — one-off reverse-engineering tools. Built on demand.

## Build and test

```bash
cargo build --release          # builds production kessel binary only
cargo test -p kessel
cargo clippy -p kessel -- -D warnings
cargo fmt --check
cargo build --release -p kessel-discovery --bin <name>  # one-off recon tool
```

## Acceptance scenarios

- Acceptance criteria are Gherkin scenarios, one per criterion, tagged `@criterion-<id>`. The runner is rstest-bdd (`rstest-bdd` and `rstest-bdd-macros` 0.6, with `rstest`, as workspace dev-dependencies).
- Scenarios are generated from the legion requirement and never hand-edited. Every `.feature` file starts with a comment saying so; to change one, change the requirement and regenerate.
- Scenarios live in a crate's `tests/`, mirroring `src/`: behaviour whose entry point is `src/a/b.rs` gets `tests/a/b.feature`, beside any `tests/a/b.rs`.
- Step code lives in the crate's `tests/bdd.rs`, which ends with `scenarios!("tests", fixtures = [world: World])` and runs every `.feature` under that crate's `tests/`. A crate that gets its first `.feature` needs its own `tests/bdd.rs` and the three dev-dependencies (`workspace = true`).
- `cargo test` runs them, in CI too. A scenario with no matching step code fails and the failure names the scenario. Cargo does not watch `.feature` files: after adding one, touch `tests/bdd.rs`.
- The `@smoke` scenario (`kessel/tests/hash.feature`) proves the wiring against the documented icon id hash.

## Run extraction

```bash
./target/release/kessel -i ~/swtor/Assets -o spice.sqlite
./target/release/kessel -i ~/swtor/Assets -o spice.sqlite --icons --icons-output ./icons
./target/release/kessel -i ~/swtor/Assets -o spice.sqlite --unfiltered
```

## Code conventions

- No `unwrap()` in library code — use `anyhow` for error propagation.
- Batch database inserts (flush at 5000 items).
- All hashing uses SHA-256 truncated to 16 hex chars.
- Icon IDs must match the frontend `computeIconId()` function.
- JSON for all data interchange (no TOML/YAML for data).
