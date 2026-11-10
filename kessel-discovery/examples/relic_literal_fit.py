"""Fit the literal relic proc magnitudes ('grant N <stat>') against cbtStandardRatingInfo and the item budget.

usage: python3 -I relic_literal_fit.py <unfiltered spice.sqlite> <cbtStandardRatingInfo.json> <spice.sqlite with item_budget_table, item_rating_table>

Literal-magnitude strings belong to abl.itm.relic.*.<quality>.ilvl_NNN abilities (and 10x/11x/12x tier names).
For the ilvl-named ones, the tier gives (quality, item level) and so the rating. For each stat family the script
reports the spread of N / column[index] for index = rating and index = item level, and whether N is a budget value
item_budget_table(quality, level - 1, permille) at one constant permille. It also lists which relic items grant
each literal ability (itmEquipAbility), to show whether live relics use them.
"""
import collections
import json
import re
import sqlite3
import statistics
import sys

unf, sr_path, db_path = sys.argv[1:4]
sr = json.load(open(sr_path))
cols = [sr["dcbfd721"], sr["dcbfd722"], sr["dcbfd723"]]
u = sqlite3.connect(f"file:{unf}?mode=ro", uri=True)
d = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
rating = {(lv, q): r for lv, q, r in d.execute("select item_level, quality, rating from item_rating_table")}
budget = collections.defaultdict(dict)
for q, lv, p, v in d.execute("select quality, item_level, permille, value from item_budget_table"):
    budget[(q, lv)].setdefault(v, []).append(p)
rows = []
for fqn, guid, txt in u.execute(
        "select o.fqn, o.guid, s.text from objects o join strings s on s.id2 = o.string_id and s.id1 = 1 "
        "and s.locale = 'en-us' where o.kind = 'Ability' and o.fqn like 'abl.itm.relic.%' and s.text glob '*grant [0-9]*'"):
    m = re.search(r"grant (\d+) ([A-Za-z ]+?) for", txt)
    k = re.search(r"\.(premium|prototype|artifact|legendary)\.ilvl_(\d+)$", fqn)
    if m and k:
        lv, q = int(k.group(2)), k.group(1)
        rows.append((m.group(2), q, lv, rating.get((lv, q)), int(m.group(1)), fqn, guid))
print("literal relic abilities with a (quality, ilvl) tier:", len(rows))
fam = collections.defaultdict(list)
for r in rows:
    fam[r[0]].append(r)
for stat, xs in sorted(fam.items()):
    print(f"== {stat}: {len(xs)} tiers, levels {min(x[2] for x in xs)}-{max(x[2] for x in xs)}")
    for mode, idx in (("rating", 3), ("item level", 2)):
        for c, col in enumerate(cols):
            ks = [x[4] / col[x[idx]] for x in xs if x[idx] is not None and x[idx] < len(col) and col[x[idx]]]
            if len(ks) > 1:
                print(f"   N / col{c + 1}[{mode}]: min {min(ks):.3f} max {max(ks):.3f} cv {statistics.pstdev(ks) / statistics.mean(ks):.3f}")
    ps = [set(budget[(x[1], x[2] - 1)].get(x[4], [])) for x in xs]
    common = set.intersection(*ps) if ps else set()
    print(f"   N in item_budget_table(q, level-1): {sum(1 for p in ps if p)} of {len(xs)}; one permille for all: {sorted(common)[:5]}")
lit = {int(r[6], 16) for r in rows}
users = collections.Counter()
for (eq,) in d.execute("select ability_guid from item_granted_abilities"):
    try:
        if int(eq, 16) in lit:
            users["granted by an item"] += 1
    except (TypeError, ValueError):
        pass
print("literal relic abilities granted by any item (item_granted_abilities):", dict(users) or 0)
