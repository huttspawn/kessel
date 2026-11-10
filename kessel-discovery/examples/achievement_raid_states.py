"""Holders of the 31 variable-shaped lgcAchievementEvents keys among hydra action and condition
values held as decimal strings (hydValue e5346faf, condition value a9189c1a).
usage: varkey_holders.py <lgcach.json> <hyd.jsonl> <out.tsv>"""
import json, sys, collections
M = (1 << 64) - 1
lg = json.load(open(sys.argv[1]))['55f64de0']
keys = {}
for e in lg:
    k = e['k']
    if isinstance(k, int):
        u = k & M
        if u >> 48 != 0xE000 and u >= 1 << 32:
            keys[u] = e['v']
want = {str(u): u for u in keys} | {str(u - (1 << 64)): u for u in keys if u >> 63}
hold = collections.defaultdict(list)


def walk(x, fqn, ev, cond):
    if isinstance(x, dict):
        name = x.get('e5346fa0')
        for fk in ('e5346faf', 'a9189c1a'):
            v = x.get(fk)
            vals = v if isinstance(v, list) else [v]
            for s in vals:
                if isinstance(s, str) and s in want:
                    hold[want[s]].append((fqn, ev, fk, name, x.get('9e1f7025') or x.get('9e1f7013')))
        for k, v in x.items():
            walk(v, fqn, ev, cond)
    elif isinstance(x, list):
        for v in x:
            walk(v, fqn, ev, cond)


for line in open(sys.argv[2]):
    o = json.loads(line)
    for e in o.get('e389e2e3') or []:
        if isinstance(e, dict):
            walk(e.get('v'), o['fqn'], e.get('k'), None)
o = open(sys.argv[3], 'w')
n = 0
for u in sorted(keys):
    h = hold.get(u)
    if not h:
        continue
    n += 1
    acts = collections.Counter((x[2], x[3]) for x in h)
    o.write('%s\t%s\t%d\t%s\t%s\n' % (hex(u), keys[u][0] if keys[u] else '', len(h), json.dumps(acts.most_common(4)), json.dumps(h[:3])))
print('keys with a hydra holder', n, 'of', len(keys))
