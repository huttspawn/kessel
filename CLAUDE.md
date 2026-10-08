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

- Runner: rstest-bdd. `cargo test` runs them; `kessel/tests/acceptance.rs` is the harness.
- `.feature` files mirror `src/`: behaviour entered at `src/a/b.rs` has its scenarios
  in `tests/a/b.feature`, step code beside it in `tests/a/b.rs`. Declare each step
  module in `tests/acceptance.rs` with `#[path]`, and add its fixture to `fixtures = [...]`.
- `.feature` files are generated from the legion requirement. Never hand-edit them.
- A step with no step code fails `cargo test`, naming the feature and scenario.
- `kessel-discovery` holds one-off tools and has no scenarios; `cargo test` at the
  root covers `kessel` only.

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
