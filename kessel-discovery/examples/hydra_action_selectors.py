"""hydObject / hydQueueObject value distribution per action name, with known names, to place an
open selector among its siblings. usage: action_selectors.py <hyd.jsonl> <action>..."""
import json, sys, collections
P = 0x100000001b3; M = (1 << 64) - 1


def f(s):
    h = 0xcbf29ce484222325
    for c in s.encode():
        h ^= c; h = (h * P) & M
    return h - (1 << 64) if h >> 63 else h


NAMES = ['INSTIGATOR', 'OBJECT SELF', 'PHASE OCCUPANTS', 'PHASE OWNER', 'EFFECT TARGET', 'INSTIGATOR GROUP',
         'INSTIGATOR COMPANION', 'PET MASTER', 'TEAMA PLAYERS', 'TEAMB PLAYERS', 'SHARED QUEST VOLUME ACTIVE',
         'INSTIGATOR GYP', 'OBJECT SELF REFERENCE', 'INSTIGATOR REFERENCE']
known = {f(n): n for n in NAMES}
acts = set(sys.argv[2:])
dist = collections.defaultdict(collections.Counter)


def walk(x, tab):
    if isinstance(x, dict):
        a = x.get('e5346fa0')
        if a in acts:
            for k in ('ed10e6a4', '9756f193'):
                v = x.get(k)
                if v is not None:
                    dist[(a, k)][known.get(v) or ('table:' + str(tab[v]) if v in tab else str(v))] += 1
        for v in x.values():
            walk(v, tab)
    elif isinstance(x, list):
        for v in x:
            walk(v, tab)


for line in open(sys.argv[1]):
    o = json.loads(line)
    tab = {e['k']: 'var' for e in o.get('ed10e6a0') or []}
    walk(o.get('e389e2e3'), tab)
for k, c in dist.items():
    print(k, c.most_common(10))
