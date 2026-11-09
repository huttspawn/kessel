"""Resolve hydra action selectors (hydObject ed10e6a4, hydQueueObject 9756f193) by:
 (1) the script's own variable table ed10e6a0 (key = fnv64(upper(short name)), name in 080ce2d2),
 (2) any script's variable table, (3) FNV-1a 64 of uppercased strings from a vocab file.
usage: hydsel.py hyd.jsonl vocab...
"""
import json, sys, collections

P = 0x100000001b3; M = (1 << 64) - 1; OFF = 0xcbf29ce484222325


def fnv(s):
    h = OFF
    for c in s.encode('latin-1', 'replace'):
        h ^= c; h = (h * P) & M
    return h - (1 << 64) if h >> 63 else h


SEL = ('ed10e6a4', '9756f193')
uses = []           # (script fqn, value)
own = {}            # fqn -> {key: (name, type)}
glob = {}
keyok = 0; keyn = 0


def walk(x, out):
    if isinstance(x, dict):
        for k, v in x.items():
            if k in SEL and isinstance(v, int) and not isinstance(v, bool):
                out.append((k, v))
            walk(v, out)
    elif isinstance(x, list):
        for v in x:
            walk(v, out)


for line in open(sys.argv[1]):
    o = json.loads(line); f = o['fqn']
    tab = {}
    for e in o.get('ed10e6a0', []) or []:
        k = e['k']; v = e['v']; n = v.get('080ce2d2', '')
        short = n.rsplit('.', 1)[-1]
        keyn += 1
        if fnv(short.upper()) == k:
            keyok += 1
        tab[k] = (short, v.get('f21c80b6', {}).get('enum'))
        glob.setdefault(k, (short, v.get('f21c80b6', {}).get('enum')))
    own[f] = tab
    out = []
    walk({k: v for k, v in o.items() if k != 'ed10e6a0'}, out)
    for k, v in out:
        uses.append((f, k, v))
print('var table keys', keyn, 'key==fnv64(upper(short name))', keyok)
names = {}
for p in sys.argv[2:]:
    for line in open(p, errors='replace'):
        s = line.rstrip('\n')
        for c in {s, s.rsplit('.', 1)[-1], s.split(':', 1)[-1]}:
            names.setdefault(fnv(c.upper()), c.upper())
for s in ['INSTIGATOR', 'OBJECT SELF', 'PHASE OCCUPANTS', 'PHASE OWNER', 'EFFECT TARGET',
          'INSTIGATOR GROUP', 'INSTIGATOR COMPANION', 'PET MASTER', 'TEAMA PLAYERS', 'TEAMB PLAYERS']:
    names[fnv(s)] = s
res = collections.Counter(); left = collections.Counter(); types = collections.Counter()
for f, k, v in uses:
    if v in own.get(f, {}):
        res['own variable table'] += 1; types[own[f][v][1]] += 1
    elif v in names:
        res['known name/vocab'] += 1
    elif v in glob:
        res['another script variable table'] += 1
    elif v == 0:
        res['zero'] += 1
    else:
        res['unresolved'] += 1; left[v] += 1
tot = sum(res.values())
print('uses', tot)
for k, c in res.most_common():
    print(f'  {k}: {c} ({100*c/tot:.1f}%)')
print('variable types of own-table hits (f21c80b6 enum)', types.most_common())
print('distinct unresolved', len(left))
for v, c in left.most_common(25):
    print('  ', v, c)
json.dump([v for v, _ in left.most_common()], open('hydsel_left.json', 'w'))
