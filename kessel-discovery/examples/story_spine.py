"""Story spine: one global quest graph with three edge kinds.

  requires    B's prerequisite gate (qstReqExpression / qstReqCondition, polarity req)
              names quest A: edge A -> B.  Input: quest_gates.tsv (RESEARCH 01a1c26d-3f4a).
  handoff     a conversation completes A (ON END, or a hook of A's last step) and starts
              B (ON START), or calls a script handler that grants B: edge A -> B.
  script      a hook in A's payload (Hydra ref 4fa14a1f + custom event cdeb4375) fires
              'On Custom:<event>' whose actions, following Execute Hydra Script, include
              Set Quest Granted B: edge A -> B.

Entry points (not edges): placeables whose use reaches Set Quest Granted B
(placeable_triggers) and trigger volumes whose script's On Enter reaches it.

usage: python3 -I story_spine.py <spice.sqlite> <qst.jsonl> <quest_gates.tsv> <outdir>
Writes edges.tsv, entries.tsv, quests.tsv to outdir.
"""
import collections
import json
import sqlite3
import sys

ON_START = 8572565631771144812
ON_END = -3973526218951927179

spice, qst_jsonl, gates_tsv, out = sys.argv[1:5]
db = sqlite3.connect(spice)

# Quest nodes and names.
quests = {f for (f,) in db.execute(
    "select fqn from objects where kind='Quest' and is_canonical=1")}
name = {}
for f, n in db.execute("select quest_fqn, name from mission_catalog where quest_fqn is not null"):
    if n:
        name[f] = n
mtype = dict(db.execute("select fqn, mission_type from quest_details"))
faction = dict(db.execute("select fqn, faction from quest_details"))

# Script handler closure: (script, handler) -> quests granted, following Execute Hydra Script.
acts = collections.defaultdict(list)
for s, h, a, t, v in db.execute(
        "select script_fqn, handler, action, target_fqn, value from script_actions order by ordinal"):
    acts[(s, h)].append((a, t, v))
memo = {}


def grants(key, seen=None):
    if key in memo:
        return memo[key]
    seen = seen or set()
    if key in seen:
        return set()
    seen.add(key)
    g = set()
    for a, t, v in acts.get(key, ()):
        if a == 'Set Quest Granted' and t:
            g.add(t)
        elif a == 'Execute Hydra Script' and v and ':' in v:
            s, e = v.split(':', 1)
            g |= grants((s, 'On Custom:' + e), seen)
    memo[key] = g
    return g


edges = collections.defaultdict(set)  # (a, b, kind) -> details


def add(a, b, kind, why):
    if a != b and a in quests and b in quests:
        edges[(a, b, kind)].add(why)


# (1) requires
for line in list(open(gates_tsv))[1:]:
    q, src, pol, tgt, var = line.rstrip('\n').split('\t')
    if pol == 'req' and tgt.startswith('qst.'):
        add(tgt, q, 'requires', f'{src} {var}')

# (3) script: quest hooks
HOOK_SCRIPT, HOOK_EVENT = '4fa14a1f', 'cdeb4375'


def walk(v, outl):
    if isinstance(v, dict):
        s, e = v.get(HOOK_SCRIPT), v.get(HOOK_EVENT)
        if isinstance(s, str) and isinstance(e, str):
            outl.append((s.lstrip('@'), e))
        for x in v.values():
            walk(x, outl)
    elif isinstance(v, list):
        for x in v:
            walk(x, outl)


hook_calls = 0
for line in open(qst_jsonl):
    o = json.loads(line)
    calls = []
    walk(o['v'], calls)
    for s, e in set(calls):
        hook_calls += 1
        for b in grants((s, 'On Custom:' + e)):
            add(o['fqn'], b, 'script', f'{s} On Custom:{e}')

# (2) handoff
last_hooks = collections.defaultdict(set)
for q, br, mx in db.execute(
        "select quest_fqn, branch, max(step) from quest_task_hooks group by 1, 2").fetchall():
    for (h,) in db.execute(
            "select hook_id from quest_task_hooks where quest_fqn=? and branch=? and step=?",
            (q, br, mx)):
        last_hooks[q].add(h)
ended = collections.defaultdict(set)
touched = collections.defaultdict(set)  # any task hook of A other than ON START
started = collections.defaultdict(set)
for c, q, h in db.execute(
        "select conversation_fqn, quest_fqn, task_hook_id from conversation_node_actions "
        "where kind='quest' and quest_fqn is not null"):
    if h == ON_START:
        started[c].add((q, 'ON START'))
        continue
    touched[c].add(q)
    if h == ON_END or (h is not None and h in last_hooks.get(q, ())):
        ended[c].add(q)
for c, s, h in db.execute(
        "select conversation_fqn, script_fqn, handler from conversation_node_actions "
        "where kind='script' and script_fqn is not null"):
    for b in grants((s, h)):
        started[c].add((b, f'script {s} {h}'))
for c in ended:
    for a in ended[c]:
        for b, how in started.get(c, ()):
            add(a, b, 'handoff', f'{c} {how}')
# touch: the looser reading (the conversation advances A at any step and starts B);
# kept apart so the strict handoff numbers stand on their own.
for c in touched:
    for a in touched[c] - ended.get(c, set()):
        for b, how in started.get(c, ()):
            add(a, b, 'touch', f'{c} {how}')

# object: a placeable that is one of A's task objects (itself or through its spawner)
# runs a script any of whose handlers grants B. This catches grants raised by systems
# other than quest hooks, conversations and On Use (for example a macrobinocular scan's
# custom event). Kept as its own kind, beside the three asked for.
by_script = collections.defaultdict(set)
for (sc, h) in list(acts):
    by_script[sc].add(h)
task_objs = collections.defaultdict(set)
for q, o in db.execute(
        "select quest_fqn, object_fqn from quest_task_objects union "
        "select q.quest_fqn, pl.entity_fqn from quest_task_objects q "
        "join placements pl on pl.spawner_fqn = q.object_fqn where pl.entity_fqn is not null"):
    task_objs[q].add(o)
plc_script = dict(db.execute(
    "select placeable_fqn, script_fqn from placeable_grants where script_fqn is not null"))
spn_script = dict(db.execute("select spawner_fqn, script_fqn from spawner_scripts"))
# A task object's own conversation (plcConvo) that starts B counts too: the KotFE
# prelude's last step is 'Use Your Ship's Mission Console', whose conversation starts
# every storyline entry (a fan-out, 'any of these next').
plc_cnv = dict(db.execute(
    "select placeable_fqn, conversation_fqn from placeable_grants where conversation_fqn is not null"))
for q, objs in task_objs.items():
    for o in objs:
        for b, how in started.get(plc_cnv.get(o), ()):
            add(q, b, 'object', f'{o} {plc_cnv[o]} {how}')
        sc = plc_script.get(o) or spn_script.get(o)
        for h in by_script.get(sc, ()) if sc else ():
            for b in grants((sc, h)):
                add(q, b, 'object', f'{o} {sc} {h}')

# Entry points
entries = set()
for p, t, d in db.execute(
        "select placeable_fqn, target_fqn, detail from placeable_triggers "
        "where kind='on_use' and detail like 'Set Quest Granted%'"):
    entries.add((t, 'placeable', p, d))
for s, area, inst in db.execute(
        "select script_fqn, area_dir_id, instance_id from trigger_volumes where script_fqn is not null"):
    for b in grants((s, 'On Enter')):
        entries.add((b, 'volume', f'{area}/{inst}', s))

with open(f'{out}/edges.tsv', 'w') as f:
    f.write('from\tto\tkind\tvia\n')
    for (a, b, k), why in sorted(edges.items()):
        f.write(f'{a}\t{b}\t{k}\t{" | ".join(sorted(why)[:3])}\n')
with open(f'{out}/entries.tsv', 'w') as f:
    f.write('quest\tsource_kind\tsource\tvia\n')
    for e in sorted(entries):
        f.write('\t'.join(map(str, e)) + '\n')
with open(f'{out}/quests.tsv', 'w') as f:
    f.write('fqn\tname\tmission_type\tfaction\n')
    for q in sorted(quests):
        f.write(f'{q}\t{name.get(q, "")}\t{mtype.get(q, "")}\t{faction.get(q, "")}\n')
kinds = collections.Counter(k for (_, _, k) in edges)
print('quests', len(quests), 'named', len(name), 'hook calls', hook_calls)
print('edges', dict(kinds), 'entries', len(entries))
