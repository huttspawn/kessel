"""Census of built-in quest-state ids (prefix48 << 16 | n, n < 16) inside Hydra scripts, by action name
and by where the id sits (action value e5346faf, or variable slot ed10e6a4 / 9756f193).

usage: python3 -I hydra_quest_state_refs.py <qst.jsonl> <hyd.jsonl> <extract.sqlite> [n_example ...]

A quest-state id is matched in both forms: prefix48 << 16 | n (expression form) and
quest string_id << 32 | n (Condition / qstVariableGUID form, string_id from objects).

hyd.jsonl: dump_kind_jsonl Hydra (top-level handler list e389e2e3, actions carry e5346fa0 name,
e5346faf value, ed10e6a4 / 9756f193 ids). Prints counts per (n, action, slot) and examples for the
n values given (default 3 5).
"""
import json, sys, collections, sqlite3

QST, HYD, DB = sys.argv[1], sys.argv[2], sys.argv[3]
SHOW = [int(x) for x in sys.argv[4:]] or [3, 5]
sid = {r[0]: r[1] for r in sqlite3.connect(DB).execute("select string_id, fqn from objects where kind='Quest' and string_id is not null")}
M = (1 << 64) - 1
prefix = {}
for line in open(QST):
    o = json.loads(line); b = o['v'].get('e4ee1382')
    if isinstance(b, int):
        prefix[(b & M) >> 16] = o['fqn']


def state(x):
    try:
        u = int(x) & M
    except (TypeError, ValueError):
        return None
    if u >= 4096 and (u >> 16) in prefix and (u & 0xFFFF) < 16:
        return prefix[u >> 16], u & 0xFFFF
    if (u >> 32) in sid and (u & 0xFFFFFFFF) < 16:
        return sid[u >> 32], u & 0xFFFFFFFF
    return None


C = collections.Counter(); ex = collections.defaultdict(list)


def walk(x, fqn, handler):
    if isinstance(x, dict):
        if 'e5346fa0' in x:
            act = x['e5346fa0']
            for slot in ('e5346faf', 'ed10e6a4', '9756f193'):
                s = state(x.get(slot))
                if s:
                    C[(s[1], act, slot)] += 1
                    if s[1] in SHOW and len(ex[(s[1], act)]) < 6:
                        ex[(s[1], act)].append((fqn, handler, s[0], x.get('e5346faf')))
        for k, v in x.items():
            walk(v, fqn, handler)
    elif isinstance(x, list):
        for v in x:
            walk(v, fqn, handler)


for line in open(HYD):
    o = json.loads(line)
    for h in o.get('e389e2e3') or []:
        walk(h.get('v'), o.get('fqn'), h.get('k'))
for k, n in sorted(C.items()):
    print(k, n)
for k, L in ex.items():
    print('==', k)
    for e in L:
        print('   ', e)
