"""Item modifier sets (#332) and the budget formula, checked against the data.

usage: python3 -I items_modsets.py <Item.jsonl> <itmModifierSetTablePrototype.json>
           <itmModifierPackageTablePrototype.json> <spice.sqlite with item_budget_table, item_stats>

Item.jsonl comes from `dump_kind_jsonl <unfiltered> Item`; the two singletons from
`dump_singleton <unfiltered> <name>`. Reports:
  1. how many items carry itmModifierSetID (acec47da) and itmModdableSetId (43f84d7ac4),
     and whether the second resolves to a key of itmModifierSets (dacef5518);
  2. whether every package id (itmModSetPackageSmallId 3534ae42aa) is a key of
     itmModifierPackagesList (4e32e7ebe8);
  3. the budget formula: for fixed-stat items, is every stat equal to
     item_budget_table(quality, item_level - 1, permille) for a package's permille,
     with control offsets 0, -2, -5 (item_budget_table.item_level is the 0-based list index).
"""
import collections
import json
import sqlite3
import sys

items_path, set_path, pkg_path, db_path = sys.argv[1:5]
items = [json.loads(line) for line in open(items_path)]
sets = {e["k"]: e["v"]["d73288aa"] for e in json.load(open(set_path))["acef5518"]}
pkgs = {e["k"]: e["v"] for e in json.load(open(pkg_path))["32e7ebe8"]}
enums = {e["name"]: e["members"] for e in json.load(open("kessel/resources/gom_enums.json"))}
STAT = enums["STAT"]

print("items", len(items), "with acec47da", sum(1 for o in items if "acec47da" in o["v"]))
moddable = [o for o in items if "f84d7ac4" in o["v"]]
print("with itmModdableSetId", len(moddable), "resolving to a modifier set",
      sum(1 for o in moddable if o["v"]["f84d7ac4"] in sets))
print("by prefix", collections.Counter(".".join(o["fqn"].split(".")[:3]) for o in moddable).most_common(8))
small = {p["34ae42aa"] for s in sets.values() for p in s}
print("package ids in sets", len(small), "that are package-table keys", len(small & set(pkgs)))
print("package variants per id", collections.Counter(len(v) for v in pkgs.values()))

db = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
budget = collections.defaultdict(dict)
for q, lvl, p, v in db.execute("select quality, item_level, permille, value from item_budget_table"):
    budget[(q, lvl)][p] = v
sigs = collections.defaultdict(list)
for key, variants in pkgs.items():
    for var in variants:
        sig = tuple(sorted((STAT[kv["k"]["enum"]], kv["v"]) for kv in var["9d4ed5c3"]))
        sigs[tuple(s for s, _ in sig)].append((key, var["d73288ae"], var["d73288b0"], dict(sig)))
stats = collections.defaultdict(list)
for f, lvl, q, s, v in db.execute(
        "select item_fqn, item_level, quality, stat_name, value from item_stats "
        "where quality in ('premium','prototype','artifact','legendary')"):
    stats[f].append((lvl, q, s, v))
for prefix in ("itm.eq.", "itm.mod."):
    group = {f: st for f, st in stats.items() if f.startswith(prefix)}
    for off in (0, -1, -2, -5):
        exact = anyp = in_range = 0
        for f, st in group.items():
            lvl, q = st[0][0], st[0][1]
            row = budget.get((q, lvl + off))
            if not row:
                continue
            vals = set(row.values())
            if all(v in vals for *_, v in st):
                anyp += 1
            for key, lo, hi, pct in sigs.get(tuple(sorted(s for _, _, s, _ in st)), ()):
                if all(row.get(pct[s]) == v for _, _, s, v in st):
                    exact += 1
                    in_range += lo <= lvl <= hi
                    break
        print(f"{prefix:9} offset {off:+d}: {len(group)} items, every stat in the row {anyp}, "
              f"one package reproduces all stats {exact} (item level inside the package's range {in_range})")
