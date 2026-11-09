"""The ship mission console's storyline entries and the gates on each entry quest.

usage: python3 -I story_spine_console.py <spice.sqlite> <quest_gates.tsv> <outdir>
Reads outdir/quests.tsv (story_spine.py); writes outdir/console.txt.
"""
import collections
import sqlite3
import sys

spice, gates_tsv, out = sys.argv[1:4]
db = sqlite3.connect(spice)
names = {}
for line in list(open(f'{out}/quests.tsv'))[1:]:
    f, n, _, _ = line.rstrip('\n').split('\t')
    names[f] = n
gates = collections.defaultdict(list)
for line in list(open(gates_tsv))[1:]:
    q, src, pol, t, v = line.rstrip('\n').split('\t')
    gates[q].append((pol, t, v, src))
rows = db.execute(
    "select conversation_fqn, node_id, quest_fqn from conversation_node_actions "
    "where conversation_fqn like 'cnv.exp.global.mission_console%' and kind='quest' "
    "order by 1, 2").fetchall()
with open(f'{out}/console.txt', 'w') as w:
    for c, n, q in rows:
        w.write(f'== {c.split("_")[-1]} node {n}: {names.get(q)} [{q}]\n')
        for pol, t, v, src in gates[q]:
            w.write(f'     {pol:3s} {v:9s} {names.get(t)} [{t}] ({src})\n')
print(len(rows), 'console entries')
