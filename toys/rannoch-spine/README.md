# rannoch-spine

Does rannoch 0.5 hold kessel's build history? One object over five builds, in a class ordered
by build number (`HistoryKey::Integer`), one fixed source. No game data.

Run: `cargo run` from this directory. Output is one line per check, PREDICTED vs ACTUAL.

Sequence: build 1 A; build 2 A (unchanged); build 3 B; build 3 C (restated correction);
build 4 C (unchanged); build 5 A (value returns); then build 0 Z (late import of an old build).

## Predictions, written before the first run

- P1 Unchanged content at a new build is an alias: 1 hearing after build 2.
- P2 Reads by build: 1=A, 2=A.
- P3 A restatement at build 3 wins: state_at(3)=C, and B remains a hearing.
- P4 The restatement does not disturb other builds: 2=A, 4=C.
- P5 A returning value: 4=C, 5=A, now=A.
- P6 A late old build changes nothing current: 0=Z, now=A.
- P7 Hearings at the end: 4 (A, B, C, Z), since aliases are not hearings.
- P8 known_at in this class is a build, not a clock, so "before the fix" cannot be asked:
  find_as_of(build 3's date, known_at=Key(3)) answers C even though B was known first.

## First run

P1 and P7 missed: an unchanged object at a new build is a new hearing, not an alias (7 hearings,
0 aliases). P2-P6 held. P8 answered C, as predicted. That run varied both the integer and the
valid time, so `identity` separates them.

## Predictions for `cargo run --bin identity`, written before its first run

The same content is sent twice, with one thing varied. Hearings after both sends:

- V1 integer class, integer differs, valid same: 2 (the integer is in the identity).
- V2 origin class, valid same, nothing else differs: 1, with 1 alias.
- V3 origin class, valid differs: 2 (valid is in the identity).
- V4 time class, time differs, valid same: 2 (the time is in the identity).

## What it showed (rannoch v0.5.0, 2026-11-01)

All four identity predictions held. A hearing's identity includes the integer, the time and the
valid time, so an alias happens only in an origin-keyed class with the same valid time.

- Sending every object at every build copies the whole build into the history. Write only what
  changed; `state_at(Key(n))` already answers the newest version at or below n.
- Reads by build, restatement at the same build, a returning value and a late old build all
  answer correctly (P2-P6).
- In an integer class `known_at` is a build, not a clock (P8): after a restatement, "as known
  before the fix" cannot be asked through `find_as_of`. Both versions stay in the log and in
  `hearings`.
- A returning value has a different identity from its first appearance, so "reverted" needs a
  content hash property written by the app.
