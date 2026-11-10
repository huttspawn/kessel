"""Who references the story-era conditions (cnd.quests.eras.*), and does anything set an era state?

usage: python3 -I era_condition_refs.py <extract.sqlite> <cnd.jsonl> <qst.jsonl> <nodes.jsonl> <hyd.jsonl> [guids_out.txt]

extract.sqlite: an unfiltered extract (objects: kind, fqn, guid). cnd/qst/hyd .jsonl: dump_kind_jsonl
Condition / Quest / Hydra; nodes.jsonl: conversation node dump ({"fqn", "node", "f"}).

1. Lists the era conditions with their GUIDs and the era fields they use (type 129 IsQuestEraInState:
   cndConditionalInteger 400003d2f2be2131 = era index, 400003d348c1ef80 = era state).
2. Counts every reference to an era condition, by fqn string or by GUID (as a hex string or an
   integer), in Conditions outside cnd.quests.eras, Quests, conversation nodes and Hydra scripts.
3. Counts Hydra and conversation action names that mention an era, and every use of the two era
   fields outside Condition clauses.
guids_out.txt (optional): one CF-prefixed GUID per line, for find_guid -g.
"""
import json, re, sys, sqlite3, collections

DB, CND, QST, NODES, HYD = sys.argv[1:6]
OUT = sys.argv[6] if len(sys.argv) > 6 else None
rows = sqlite3.connect(DB).execute("select fqn, guid from objects where fqn like 'cnd.quests.eras.%'").fetchall()
era = {f: g.upper() for f, g in rows}
needles = set(era)
for g in era.values():
    v = int(g, 16)
    needles |= {g, g.lower(), str(v), str(v - (1 << 64))}
print('era conditions', len(era))
fields = collections.Counter(); internal = 0
ERA_FIELDS = ('f2be2131', '48c1ef80')
for line in open(CND):
    o = json.loads(line)
    hit = [n for n in needles if n in line]
    if o['fqn'] in era:
        g = era[o['fqn']]; v = int(g, 16)
        own = {o['fqn'], g, g.lower(), str(v), str(v - (1 << 64))}
        internal += any(n not in own and not any(n in s for s in own) for n in hit)
        for c in o['v'].get('04ee5c1e') or []:
            t = c.get('04ee5c20'); t = t['enum'] if isinstance(t, dict) else 0
            if t == 129:
                fields[(json.dumps(c.get('f2be2131')), json.dumps(c.get('48c1ef80')))] += 1
    elif hit:
        print('  CONDITION REF', o['fqn'], hit[:2])
print('era clauses (era index, state):', sorted(fields.items(), key=lambda x: (str(x[0][0]), str(x[0][1]))))
print('era conditions that reference another era condition (nesting):', internal)
refs = collections.Counter(); ex = collections.defaultdict(list)
era_word = re.compile(r'(^|[^a-z])era([^a-z]|$)', re.I)
acts = collections.Counter(); field_uses = collections.Counter()


def actions(x, src):
    if isinstance(x, dict):
        a = x.get('e5346fa0')
        if isinstance(a, str) and era_word.search(a):
            acts[(src, a)] += 1
        for k, v in x.items():
            if k in ERA_FIELDS:
                field_uses[(src, k)] += 1
            actions(v, src)
    elif isinstance(x, list):
        for v in x:
            actions(v, src)


for src, path in (('Quest', QST), ('Conversation', NODES), ('Hydra', HYD)):
    for line in open(path):
        hit = [n for n in needles if n in line]
        o = json.loads(line)
        if hit:
            refs[src] += 1
            if len(ex[src]) < 5:
                ex[src].append((o.get('fqn'), o.get('node'), hit[:2]))
        actions(o, src)
print('objects referencing an era condition:', {s: refs[s] for s in ('Quest', 'Conversation', 'Hydra')})
for s, L in ex.items():
    for e in L:
        print('  ', s, e)
print('action names mentioning an era:', dict(acts) or 'none')
print('era fields used outside Condition clauses:', dict(field_uses) or 'none')
if OUT:
    open(OUT, 'w').write('\n'.join('CF' + g for g in sorted(set(era.values()))) + '\n')
