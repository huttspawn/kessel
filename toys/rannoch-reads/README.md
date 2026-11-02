# rannoch-reads

How fast can a relay load kessel's real nodes from rannoch? Loads real rows from spice
(Npc, Item, Quest, Ability: about 168,000 nodes with their JSON as one property) into a store
at rannoch main 2bac9b5, links each located NPC to its planet, restates 10% of NPCs at build 2,
then walks planets the way a relay seeds: `links(planet, on, In)`, then `state_at` per node.

Run: `python3 -I export.py <spice.sqlite> <bodies.jsonl>`, then
`cargo run --release -- <bodies.jsonl> <dir for the store>`.

## Predictions, written before the first run

- P1 Load and fill: 15,000-35,000 events a second.
- P2 `links` for Hutta (413 NPCs): under 5 ms.
- P3 `state_at` per node: p50 100-200 us, p99 under 1 ms. Sub-ms holds, but not by much.
- P4 A full Hutta seed (links plus every node): 50-100 ms, inside warden's 1-3 s budget.
- P5 The largest planet walk costs the same per node as Hutta: no growth with fan-out.
- P6 A quarter or more of `state_at` is ontology loading, since each read calls
  `current_ontology`. Checked by comparing against `find` and a profile if P3 misses.

## First run (rzr, encrypted NVMe-class home volume, store just written, so in page cache)

P1 15,700 events/s, the bottom of the range. P2 held: 1.5 ms for Hutta, 3.6 ms for 1,465 nodes.
P3 and P4 were far too pessimistic: `state_at` p50 about 10 us, p99 under 20 us; a Tatooine seed
of 1,465 nodes takes 18 ms. P5 held. P6 was not checked, since P3 did not miss. The store is
987 MB for 278 MB of bodies, 3.5 times.

## Predictions for `cold` (`cargo run --release --bin cold -- <store> <class> <limit>`), before its first run

The file is evicted from the page cache with posix_fadvise first; a new process opens it.

- C1 First Hutta walk, cold: p50 100-500 us, p99 2-10 ms, 413 nodes in 100-400 ms.
- C2 Second walk in the same process: back to about 10 us.
- C3 Tatooine straight after, still cold for its own pages: similar per node to C1.
- C4 Quests (9 KB average, largest 520 KB), warm: p50 20-40 us, max a few ms.

## Cold run

C2 held. C1 and C3 beat the predictions: on an evicted file, p50 175 and 109 us, p99 404 and
279 us, every read under 1 ms; Hutta 109 ms, Tatooine 238 ms. `links` is the slow part cold (31
and 45 ms). C4: quests p50 11 us, p99 55 us, max 805 us. Open took 1.4 ms.

## Not measured

No writer running beside the reads. 168,000 of 421,000 objects. Conversations left out (88 KB
average, 2.6 MB largest). Nearly every node has one version. No keep filter.

## Predictions for `restate` (`cargo run --release --bin restate`), before its first run

Issue #171's model, one class on axis (build), compared with `toys/rannoch-spine` P8:

- R1 Build 3 now answers the correction, C.
- R2 Build 3 as known before the correction answers B, which v0.5.0 could not ask.
- R3 Build 2 is untouched, A, at either known-at.
- R4 A read of the point (build 3) known before B was written answers A, the newest then known.

## Restate run

R1-R4 held at rannoch main 2bac9b5. Under #171 a correction at one build can be read as known
before the fix, the gap `toys/rannoch-spine` P8 found in v0.5.0.

The `restate` bin needs rannoch main; `bin/cold` takes the store file `rannoch-reads` wrote.
Evict it first with `posix_fadvise(POSIX_FADV_DONTNEED)`.
