# rannoch-axis

Re-runs `toys/rannoch-spine`'s restatement case under the proposal in bullpen 01a1986f: known-at
is clock time in every class, a class declares its axis, and amendments are allowed in
integer-keyed classes. It also tries the corners that post asks about.

It runs against rannoch v0.5.0 with `known-at.patch` applied, and never against a live store.
The patch makes known-at a clock in integer classes, lifts the amendment refusal, and adds the
missing read `state_known_at(node, at, known)`. It leaves Time classes as they are, so T below
measures what v0.5 does there. A compound axis is not built: C packs `(build, step)` into one
integer, `build << 16 | step`, which gives the same order.

Run: `./run.sh`. Output is one line per check, PREDICTED vs ACTUAL.

## Predictions, written before the first run

Sequence for N: build 1 A; build 2 A; build 3 B; (mark 1); build 3 C, the fix; (mark 2); build 4 C;
build 5 A; (mark 3); build 0 Z, a late old build; (mark 4). Same writer throughout.

Restatement under clock known-at:

- N1 Key(3) known at mark 1 = B. The read kessel could not make before.
- N2 Key(3) known now = C.
- N3 Key(2) known at mark 1 = A, and known now = A.
- N4 Key(0) known at mark 3 = none, known now = Z. A late axis value is not known before it landed.
- N5 Key(5) now = A, and find = A: the late build does not change what is current.
- N6 find_as_of(build 3's date, known at mark 1) = B.
- N7 find_as_of with known-at Key(3) is refused: known-at is no longer a build.

Amendment in an integer class (on a separate node):

- A1 Write build 3 C; amend it to C2. Key(3) before the amendment = C, after = C2.
- A2 Amending does not add a hearing: 1 hearing after it.
- A3 An amendment patching `data.integer` to 9 lands and does not move the version: Key(3) still
  answers it. A silent no-op, which the store should refuse.
- A4 An amendment removing `data.integer` is refused and stops; Key(3) keeps its value.

A correction that reverts (on a separate node):

- R1 Build 3 A, B, then A again. The third is an alias of the first: 2 hearings, 1 alias.
- R2 Key(3) known between B and the revert = B; now = A.

Replay into a second store, opened under another content key, every body sent unchanged in one
batch, in log order:

- W1 N1 through N6 answer the same as the first store, including "known at mark 1 = B", although
  this store recorded B and C in the same batch, after every mark.
- W2 Event ids of the hearings are equal across the stores; content identities differ.
- W3 The amendment from A1 does not replay: its subject is a log id local to the first store. It
  stops as refused, and Key(3) on that node reads C here and C2 there.
- W4 Sending the batch again changes nothing: same hearing count, no aliases added.
- W5 Sending the batch in reverse log order into a third store answers N1-N6 the same.
- W6 Re-minting: the old B sent again under a new id (what a rebuilt store's export does) is an
  alias at a later clock, so Key(3) now = B. The fix is silently undone. Stable ids are required.

Write-only-what-changed meets a restatement:

- D1 The truth is build 3 C, build 4 B. The old decoder said B at 3 and B at 4, so kessel wrote only
  3=B. The fix restates 3=C. Key(4) now = C, which is wrong.
- D2 Re-diffing the later builds writes 4=B. Key(4) now = B; Key(4) known before that write = C.

Compound axis, packed (build, step), all at build 3 unless named:

- C1 Steps 0=B, 1=C, 2=D, then step 1 again =E, recorded last. Read at (3, top) = D: the newest
  recording loses to the higher step.
- C2 Read at (3, 0) = B: a partial point read with step 0 misses steps 1 and 2. A partial point must
  mean the top of its prefix.
- C3 Read at (3, 1) known before E = C; now = E.
- C4 A tie at (4, 0), F then G: G wins by clock. The replay in W answers the same.

Key part missing, and changing the axis:

- M1 A node of an integer class with no `data.integer` is refused and stops; no hearing.
- M2 Declaring the class again as Time is refused.
- M3 Declaring it again unchanged writes nothing.

Time class, unpatched:

- T1 A Time-class node with `data.time` a week old, recorded now. Known at a clock three days ago
  answers it, although nothing held it then. Known-at in a Time class is the writer's time, so
  proposal 1 is not finished by the integer-class change.
- T2 An Origin-class node, same read: none. Its point is the clock.

Links on the build axis:

- L1 link_state_at with At::Key is refused.
- L2 A link event carrying `data.integer` has it ignored: the same link sent at "build 3" and
  "build 4" is 1 hearing, 1 alias, and neither hearing shows a build.

## What it showed (rannoch v0.5.0 + known-at.patch, 2026-11-01)

Every prediction held on the first run. They were written after reading the store's code, so this
shows the code reads as expected, not that the design is proven. Not tested: a native compound
axis, links under the patch, the queue path, clock skew between hosts, a Time-class fix.

- Clock known-at closes the gap: build 3 as known before the fix answers B (N1, N6). A late old build
  is not known before it landed (N4), which an integer known-at could not say either.
- The change for integer classes is small because `point` already holds the id's clock. Time classes
  need more: `point` is the writer's time there, so T1 still answers a week-old write as known three
  days ago, before it was recorded.
- Replay holds when known-at is the clock in the event id: another store, another key, one batch,
  redelivered, or reversed, all answer the same (W1, W4, W5).
- Amendments do not replay. The subject is a log id minted per store, so the second store stops them
  and keeps the old value (W3). The stores differ from then on.
- Re-minting undoes a fix: the old B under a new id wins at build 3 (W6). Ids must be kept.
- A restatement under write-only-what-changed leaves later builds wrong until they are re-diffed
  (D1, D2). That is kessel's job, not rannoch's.
- A step can contradict the clock: the newest correction at a lower step loses (C1). A partial point
  must mean the top of its prefix (C2). Same-build order already falls to clock, then event id (N2).
- A patch of `data.integer` lands and moves nothing (A3); it should be refused.
- A relation has no axis: At::Key is refused and a link's `data.integer` is dropped (L1, L2).
