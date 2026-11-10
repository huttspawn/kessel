"""What are a modifier package's variants? Test: an item's stats = the SUM over the package's
variants of item_budget_table(quality, item_level - 1, variant permille), i.e. each variant is one
component (armoring / mod / enhancement) of the same piece.

usage: python3 -I items_package_variants.py <itmModifierPackageTablePrototype.json> <spice.sqlite>

Reports, for fixed-stat items (item_stats) in the four budget qualities, how many are reproduced
exactly by (a) one variant alone, (b) the sum of all variants of one multi-variant package, at
list index item_level - 1 and at control offsets.
"""
import collections
import json
import sqlite3
import sys

pkg_path, db_path = sys.argv[1:3]
enums = {e["name"]: e["members"] for e in json.load(open("kessel/resources/gom_enums.json"))}
STAT = enums["STAT"]
pkgs = {e["k"]: e["v"] for e in json.load(open(pkg_path))["32e7ebe8"]}
db = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
budget = collections.defaultdict(dict)
for q, lvl, p, v in db.execute("select quality, item_level, permille, value from item_budget_table"):
    budget[(q, lvl)][p] = v
multi = {}
for key, variants in pkgs.items():
    vs = [{STAT[kv["k"]["enum"]]: kv["v"] for kv in var["9d4ed5c3"]} for var in variants]
    if len(vs) > 1 and all(vs):
        multi[key] = (vs, variants[0]["d73288ae"], variants[0]["d73288b0"])
ranges = collections.Counter(len({(v["d73288ae"], v["d73288b0"]) for v in pkgs[k]}) for k in multi)
print("multi-variant packages", len(multi), "distinct level ranges per package", dict(ranges))
by_names = collections.defaultdict(list)
for key, (vs, lo, hi) in multi.items():
    names = frozenset(s for v in vs for s in v)
    by_names[names].append(key)
stats = collections.defaultdict(dict)
meta = {}
for f, lvl, q, s, v in db.execute(
        "select item_fqn, item_level, quality, stat_name, value from item_stats "
        "where quality in ('premium','prototype','artifact','legendary')"):
    stats[f][s] = v
    meta[f] = (lvl, q)
for off in (-1, 0, -2):
    hit = collections.Counter()
    ex = {}
    for f, st in stats.items():
        lvl, q = meta[f]
        row = budget.get((q, lvl + off))
        if not row:
            continue
        for key in by_names.get(frozenset(st), ()):
            vs, lo, hi = multi[key]
            tot = collections.Counter()
            ok = True
            for v in vs:
                for s, p in v.items():
                    if p not in row:
                        ok = False
                    tot[s] += row.get(p, 0)
            if ok and dict(tot) == st:
                pre = ".".join(f.split(".")[:3])
                hit[(pre, lo <= lvl <= hi)] += 1
                ex.setdefault(pre, (f, lvl, q, st, key, vs))
                break
    print(f"offset {off:+d}: items reproduced by the sum of a package's variants:",
          sum(hit.values()), dict(hit.most_common(12)))
    if off == -1:
        for pre, e in list(ex.items())[:6]:
            print("   e.g.", e)
