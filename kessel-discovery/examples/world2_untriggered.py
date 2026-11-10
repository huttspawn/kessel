#!/usr/bin/env python3
"""Decode push 2, world: the placed use-placeables that have no placeable_triggers row.

A placed placeable (placements.entity_fqn) with a use ability
(placeable_grants.use_ability_fqn) and no row in the placeable_triggers view.
Classifies each by what its script (own, or its spawner entry's) contains,
whether that script exists in script_actions, and what its use ability is.

Usage: world2_untriggered.py <spice.sqlite> <unfiltered.sqlite> <out_dir>
"""
import collections
import sqlite3
import sys

db = sqlite3.connect(sys.argv[1])
un = sqlite3.connect(sys.argv[2])
out = sys.argv[3]

rows = db.execute(
    "select distinct pl.entity_fqn, g.use_ability_fqn, g.script_fqn, g.codex_fqn, g.conversation_fqn "
    "from placements pl join placeable_grants g on g.placeable_fqn = pl.entity_fqn "
    "where g.use_ability_fqn is not null "
    "and pl.entity_fqn not in (select placeable_fqn from placeable_triggers)").fetchall()

spn_script = collections.defaultdict(set)
for p, s in db.execute(
        "select distinct pl.entity_fqn, ss.script_fqn from placements pl "
        "join spawner_scripts ss on ss.spawner_fqn = pl.spawner_fqn"):
    spn_script[p].add(s)
handlers = collections.defaultdict(collections.Counter)
for s, h, a in db.execute("select script_fqn, handler, action from script_actions"):
    handlers[s][(h, a)] += 1
in_spice = {f for (f,) in db.execute("select fqn from objects where kind='Hydra'")}
in_unf = {f for (f,) in un.execute("select fqn from objects where fqn like 'hyd.%'")}
abl_spice = {f for (f,) in db.execute("select fqn from objects where fqn like 'abl.%'")}
abl_unf = {f for (f,) in un.execute("select fqn from objects where fqn like 'abl.%'")}
qto = {o for (o,) in db.execute("select object_fqn from quest_task_objects")}
spawners = collections.defaultdict(set)
for p, s in db.execute("select distinct entity_fqn, spawner_fqn from placements"):
    spawners[p].add(s)
uses = collections.Counter(p for (p,) in db.execute(
    "select entity_fqn from placements where entity_fqn is not null"))

cls = collections.Counter()
abl = collections.Counter()
lines = []
for p, ab, sc, cdx, cnv in rows:
    scripts = ({sc} if sc else set()) | spn_script.get(p, set())
    if not scripts:
        k = "no script (should have a use_ability row)"
    else:
        hs = collections.Counter()
        missing = []
        for s in scripts:
            if s not in in_spice:
                missing.append(s + (" (unfiltered only)" if s in in_unf else " (not in client)"))
            hs.update(handlers.get(s, {}))
        if missing and not hs:
            k = "script missing: " + ("unfiltered only" if all("unfiltered" in m for m in missing) else "not in client")
        elif not hs:
            k = "script has no decoded actions"
        else:
            on_use = {a for (h, a) in hs if h == "On Use"}
            if not on_use:
                k = "script has no On Use handler"
            else:
                k = "On Use only presentation: " + ",".join(sorted(on_use))
    cls[k] += 1
    abl[ab.rsplit(".", 1)[0] if ab else None] += 1
    a_where = "spice" if ab in abl_spice else ("unfiltered" if ab in abl_unf else "absent")
    task = bool(qto & ({p} | spawners.get(p, set())))
    lines.append((k, p, ab, a_where, ";".join(sorted(scripts)), str(uses[p]), str(task)))

with open(f"{out}/untriggered.tsv", "w") as f:
    f.write("class\tplaceable\tuse_ability\tability_in\tscripts\tplacements\tquest_task_object\n")
    for l in sorted(lines):
        f.write("\t".join(map(str, l)) + "\n")
print("placeables", len(rows))
for k, v in cls.most_common():
    print(" ", v, k)
print("use ability families", abl.most_common(15))
print("ability present in", collections.Counter(l[3] for l in lines))
