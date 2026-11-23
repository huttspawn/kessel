# Results (rzr, 2026-11-05 and 06, rannoch v0.6.0)

Builds: live Steam 26435744 as build 1, PTS 25812348 as build 2, PTS 26487381 (a 658 MB patch,
installed during the run) as build 3, all on one graph. Data: the session scratchpad's `shape/`.

## Load (one droppable directory per build)

| | live (1) | PTS (2) | PTS (3) |
|---|---|---|---|
| objects | 726,576 | 750,882 | 756,460 |
| seconds | 84 | 79 | 53 |
| raw payloads | 1,252 MB | 1,277 MB | 1,284 MB |
| `pieces.bin` (zstd per piece) | 364 MB | 377 MB | 380 MB |
| inbox (thin events) | 350 MB | 361 MB | 366 MB |

P1 missed high (726,576, not 400-450K; spice holds 501,027 distinct names because it filters).
P2 held at the low end. P3 missed: the inbox costs as much as the bytes. P4 held.

The first loader silently dropped all 10,752 conversations: they are standalone `PROT` entries, not
bucket objects. Found only by diffing names against spice. A load must check itself against an
independent census every time.

Census of the live install by first bytes (`census`): media dominates (DDS 70 GB raw, GAWB 7.7, JAWB
5.4, audio 5). Area and room files (`18 00 00 00`) are 3.5 GB raw, 2.2 GB stored, and are not in this
toy's load yet; nor are string tables and the XML entries.

## Work (kessel's worker into the branch graph, build axis)

- Build 1: 726,576 nodes written, 0 unread, 143-160 s (about 5,000 objects a second; P5 held at the
  low end). 2,455 MB of JSON became a 9.8 GB graph before links, 4x (P6 held).
- Build 2: bytes differ on 31% of objects (every item, NPC, quest and condition carries a per-build
  difference), but canonical rendering wrote 8,053 changed, 25,922 added and 1,616 removed: 95.5%
  wrote nothing (P7 held). Graph +540 MB on nodes (P8 held).
- Build 3: 145,000 objects differ by bytes; 9,859 changed, 7,053 added, 1,475 removed were written
  (2.4%). About +480 MB with links. Build 2's digest is unchanged after build 3.
- No GUID moved between any two of the three builds (FR-KESSEL-020 assumes they move).
- Repeat (P10): the same bytes under a renamed reader wrote nothing.

## Links (a second worker on the graph's own log)

2,158,935 links at build 1 across 190 relations, one per class pair (P9 held). 2,353,914 at build 2,
2,416,719 at build 3. Build 3's links: +67,884, ended 5,079, 12 s. A relation can be declared only
with the queue closed: 182 reopens on the first pass.

## Reader fix (P12, P13)

The older reader (`read_object_fields`) cannot read 46,135 of 726,576 live objects (6.3%: abilities,
NPCs, paths, placeables) and reads every conversation's tail as empty; where both read, the fields
agree exactly. Built with the old reader, then corrected with the framed one over both builds in
order: the fix wrote only those objects, at the same builds, and the corrected graph's digests equal
a clean run's at both builds, nodes and links. Read as known before the fix, a conversation answers
the old reader's version; now, the framed one's.

## Crashes (P11)

- Nodes, 47 kills: exact. 726,576 node events, each once, digest equal to the clean run.
- Toy bugs found by kills: a worker on a second file must close inbox events only after the graph
  committed their writes (2,558 objects lost before the fix), and anything computed from "what this
  run saw" is wrong after a resume (the removal pass marked 724,000 objects removed).
- Links, 25 kills: 1,694 links short. A worker's place, saved by the heartbeat, can pass a node
  whose links are still held. Fixed by closing only after a flush; the rerun is `exp4.sh`.

## Neighbourhoods (FR-KESSEL-013, depth 3, hub cap 200)

Measured while other runs used the disk, through `describe` plus `links` per relation (no relay
shape can name every relation). Objects p50/p90: abl 12/99, npc 288/983, itm 17/218, qst 138/990,
cnv 7/155 (its tail is not decoded, so it has almost no links), hyd 7/141. Milliseconds p50: 3 to
580, p90 up to 2.3 s. Too slow for a warm working set as written; not measured alone.
