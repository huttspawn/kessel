# rannoch-shape

Can kessel live in rannoch v0.6.0's shape? Real objects from the live and PTS installs go through
it end to end:

- `load`: one droppable directory per build. `pieces.bin` holds each object's header and payload,
  zstd-compressed one by one; `inbox.rannoch` holds one thin event per object (kind, name, guid,
  hash, offset, lengths), of an app type that fills no table. Nothing is decoded or filtered.
- `work`: kessel's own worker takes the inbox, decodes each object, renders it canonically (every
  id that names an object in this build becomes its FQN, maps sorted, bytes after the field stream
  kept whole) and compares it with what the branch graph holds at that build. Only a difference
  is written, on a `build` axis, committed with the inbox position. References become links, and
  a reference that goes away ends its link. An object the build no longer holds is marked removed.
  Bytes that do not decode become a node of class `unread`, with the reason.
- `check`: reads the graph across builds and as known before a fix.

Live is build 1 and PTS is build 2 on one graph: they are not one branch, but they are two real
builds with a real difference. Run `./run.sh`; results are in RESULTS.md.

## Predictions, written before the first run

Sizes and speed:

- P1 Objects: 400,000-450,000 per build (spice holds about 421,000).
- P2 `pieces.bin`: 1-3 GB per build; about 3 to 4 times smaller than the raw payloads.
- P3 Inbox: 200-400 bytes an event in the log, plus SQLite overhead, so 150-300 MB per build.
- P4 Load: under 15 minutes per build; most of it reading 55 GB of archives.
- P5 Worker into an empty graph: 5,000-20,000 objects a second, slower than rannoch's 90,000 fill,
  because bodies are far larger and each node is compared first.
- P6 Graph after build 1: 3 to 5 times the JSON it holds, as toys/rannoch-reads measured 3.5.

Changes:

- P7 Canonical rendering makes build 2 write far fewer objects than differ by raw hash. Raw
  payload hashes differ on over 50% of objects (the per-build generation byte and NPC counter).
  Canonical nodes written at build 2: under 15%, unless those counters are decoded fields, which
  would show up as one field id changing on nearly every object of a kind.
- P8 Graph growth at build 2 is under 25% of its size after build 1.
- P9 Links: 2-6 million after build 1, and more than 100 relations, since each pair of classes
  needs its own. That many relations makes walking any neighbourhood awkward.

Correctness:

- P10 Running the worker again on the same build writes nothing.
- P11 `kill -9` during `work`, then run again: the graph answers the same as a clean run, with no
  object lost and none written twice, because the position commits with the events.
- P12 Reader fix: the older reader (`read_object_fields`, which skips leading zeros) fails or
  differs on at least 1% of objects. A second pass with the framed reader writes only those objects,
  at the same builds. Read as known before the fix, the old answer stays; read now, the new one.
- P13 A fix at build 1 leaks into build 2 for objects build 2 did not rewrite, until the pass reaches
  build 2, and the pass in build order leaves every object right at both builds.
- P14 Failures: with the framed reader under 0.1% of objects go to `unread`, none silently.
