# Field registry

`registry.jsonl` has one line for every GOM field id in the game data: 4,078 across 726,576 objects and singletons. Each says what the field is, how it was settled, where it is used, its value shapes, and what it points at.

| Status | Fields | How it was settled |
|---|---:|---|
| `named` | 3,096 | A name proved by its hash in `client.gom` |
| `decoded-proved` | 347 | The data shows it: a join resolves or values match a known quantity. `evidence` says how |
| `decoded-likely` | 191 | The values fit, but nothing excludes another reading |
| `points-at` | 130 | Its values reference objects of a known kind |
| `container` | 34 | It holds other fields, which have their own lines |
| `unsettled` | 280 | The data cannot settle it. `reason` says why and what would |

Names are for reading only. The wire format and most names rest on earlier community work, chiefly the PugTools project, read as a reference and checked here against the data. Extraction reads fields by id and never opens this folder. A name's letters are proved by hash; `name_proposed_by` says who proposed it, and most came from a community list. 565 field ids are not in `client.gom` at all, so a hash cannot prove anything about them: they are settled from the data or left unsettled.

## Rerun

```sh
cargo run --release -p kessel-discovery --example field_sweep -- <unfiltered spice.sqlite> > fields.jsonl
python3 -I kessel-discovery/examples/field_registry.py fields.jsonl docs/fields/names.json docs/fields/registry.jsonl docs/fields/findings-*.json
```

`findings-*.json` hold the decoding by hand, one file per system: field id to `meaning`, `evidence` and `confidence`, or `reason`. A new field in a later build shows as status `open` until someone settles it.
