"""Resolve the non-E000 achievement event keys: find each as a value anywhere in the jsonl dumps
(quest/hydra variable GUIDs and the like), reporting the field id and the enclosing object."""
import json, sys, collections

M = (1 << 64) - 1
lg = json.load(open('../lgcach.json'))['55f64de0']
keys = {}
for e in lg:
    k = e['k']
    if isinstance(k, int):
        u = k & M
        if u >> 48 != 0xE000:
            keys[u] = e['v']
want = set(keys) | {k - (1 << 64) for k in keys if k >> 63}
found = collections.defaultdict(list)


def walk(x, fqn, field, parent):
    if isinstance(x, bool):
        return
    if isinstance(x, int):
        if x in want:
            found[x & M].append((fqn, field, parent))
    elif isinstance(x, dict):
        name = None
        for k in ('080ce2d2', '2adec3c7'):
            if isinstance(x.get(k), str):
                name = x[k]
        for k, v in x.items():
            if k in ('k', 'v'):
                walk(v, fqn, field, name or parent)
            else:
                walk(v, fqn, k, name or parent)
    elif isinstance(x, list):
        for v in x:
            walk(v, fqn, field, parent)


for p in sys.argv[1:]:
    for line in open(p):
        o = json.loads(line)
        walk(o.get('v', o), o.get('fqn'), None, None)
res = collections.Counter()
for u in sorted(keys):
    f = found.get(u, [])
    res['small' if u < 1 << 32 else ('found' if f else 'notfound')] += 1
    if u >= 1 << 32:
        print(hex(u), keys[u][:1], f[:3])
print(res)
