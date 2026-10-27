# EPP / FXSpec encoding and the dict-free fxspec set

Issue: #294 (Phase 4 of #290, dictionary-free extraction).
Measured 2026-10-27 against the live `~/swtor/Assets` archives and the
local `hashes_filename.txt`. Tool:
`kessel-discovery/examples/probe_epp_fxspec_encoding.rs`.

## TL;DR

- The "~22k non-BOM `.epp`/`.fxspec` entries" from the census are `.fxspec`
  files: UTF-16-LE **without** a BOM. kessel already decodes every one of
  them (`decode_fxspec` accepts BOM-less UTF-16-LE). They are not a gap.
- Only **9** `.epp` entries lack the BOM. They are real `<Appearance>` specs
  that `decode_epp` rejects today, with or without the dictionary.
- Dict-free `fx_specs` (paths derived from `appearance_specs.fx_spec_refs`)
  yields **8,044** rows, byte-identical to the matching rows of a dict-backed
  run. The dict-backed run has **22,595**; the other 14,551 fxspecs are
  referenced by no `.epp` and cannot be named without the dictionary.

## Encoding census (dict-named entries)

| ext | leading bytes | entries | decoded by kessel |
|---|---|---|---|
| `.epp` | `FF FE` (UTF-16-LE BOM) | 20,646 | 20,646 |
| `.epp` | `3C 00` (UTF-16-LE, no BOM) | 9 | 0 |
| `.fxspec` | `FF FE` (UTF-16-LE BOM) | 2 | 2 |
| `.fxspec` | `3C 00` (UTF-16-LE, no BOM) | 22,591 | 22,591 |
| `.fxspec` | `3C` (UTF-8) | 2 | 2 |

No entry is binary; every one is XML in UTF-16-LE or UTF-8.

The 9 BOM-less `.epp` files:

```
/resources/gamedata/epp/agent/stealth/loop.epp
/resources/gamedata/epp/npc/ability/tech/fire_bomb/cast.epp
/resources/gamedata/epp/placeables/open/push_loop.epp
/resources/gamedata/epp/placeables/sabotage/sabotage_mid_end.epp
/resources/gamedata/epp/placeables/sabotage/sabotage_mid_loop.epp
/resources/gamedata/epp/racials/twilek/dance_male.epp
/resources/gamedata/epp/test/jwalker/bvttest20080618a.epp
/resources/gamedata/epp/test/jwalker/bvttest20080618w.epp
/resources/gamedata/epp/test/jwalker/bvttest20080619b.epp
```

Each starts `<Appearance fqn="epp...." GUID=...` in UTF-16-LE. They fail
because `xml_utf16::decode_xml_bom` falls back to UTF-8 when there is no BOM,
and `is_epp` (the #292 sniff) requires the BOM, so they are missing from
`appearance_specs` in both dict-backed and dict-free runs.

## Go / no-go on the non-BOM residual

- `.fxspec` non-BOM (22,591): **no-go**, nothing to do. Already decoded.
- `.epp` non-BOM (9): **go, low priority.** They carry `appearance_specs`
  rows kessel would store (three are 2008 test files; six are live:
  agent stealth loop, NPC fire bomb, two sabotage placeables, a push loop,
  a Twi'lek dance). The fix is a BOM-less UTF-16-LE branch in the EPP sniff
  and decode, the same shape `decode_fxspec` already has. Out of scope here.

## Dict-free fxspec derivation

`fx_spec_refs` are written freely, so a ref is normalized before it is
hashed: lowercased, `\` turned into `/`, a leading `/` and a trailing
`.fxspec` dropped, then `/resources/art/fx/fxspec/<ref>.fxspec`. All 23,331
dict `.fxspec` paths sit under that one prefix and are lowercase.

| measure | count |
|---|---|
| distinct refs across `appearance_specs` (normalized) | 8,149 |
| refs that hit an archive fxspec | 8,044 |
| refs that hit nothing (dangling, e.g. `audio_weapon_swingright_normal`) | 105 |
| exact-string refs that hit (no normalization) | 7,457 |
| dict-backed `fx_specs` rows | 22,595 |
| dict-free `fx_specs` rows | 8,044 |

Every dict-free row matches its dict-backed row on `fqn`, `node_classes_json`
and `raw_xml`; no dict-backed fxspec reachable from a ref is missing.

The 14,551-row delta is fxspecs no appearance spec names. Phase 5 (dropping
the dictionary) loses them unless another self-reference is found (for
example fxspec-to-fxspec refs inside `marshalData`, not measured here).
