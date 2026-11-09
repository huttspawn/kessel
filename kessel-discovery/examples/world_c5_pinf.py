"""C5: what the PINF registry holds that kessel never loads.
1. PINF flag by loaded object kind (unfiltered).
2. Referenced-but-unloaded FQN classes (pkg, wpn, med, sce, veh, missing hyd): fold each FQN to a GUID and look it up in PINF.
usage: c5_pinf.py <unfiltered> <pinf.tsv> <missing_scripts.txt>"""
import collections, json, re, sqlite3, sys

un = sqlite3.connect('file:' + sys.argv[1] + '?mode=ro', uri=True)
pinf = {}
for l in open(sys.argv[2]):
    g, f = l.split()
    pinf[int(g, 16)] = f


def fnv(s):
    h = 0xcbf29ce484222325
    for b in s.upper().encode():
        h ^= b
        h = (h * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return h


def fold(fqn):
    h = fnv(fqn)
    return 0xE000000000000000 | (h & 0x0000FFFFFFFF0000) | ((h ^ (h >> 48)) & 0xFFFF)


kinds = collections.defaultdict(collections.Counter)
loaded = set()
pre = collections.defaultdict(collections.Counter)
refs = collections.Counter()
for fqn, kind, guid, canon, js in un.execute("select fqn, kind, guid, is_canonical, json from objects"):
    g = int(guid, 16) if guid else None
    if canon:
        loaded.add(g)
        kinds[kind][pinf.get(g, 'absent')] += 1
        pre[fqn.split('.')[0]][pinf.get(g, 'absent')] += 1
    try:
        for s in json.loads(js).get('strings') or []:
            m = re.fullmatch(r'(pkg|wpn|med|sce|veh|hyd|cnv|stg|creature|nco|npc|plc|spn|enc)\.[a-z0-9_.]+', s)
            if m:
                refs[s] += 1
    except Exception:
        pass
print('fold check on loaded canonical objects:',
      sum(1 for fqn, guid in un.execute("select fqn, guid from objects where is_canonical=1 limit 5000")
          if fold(fqn) == int(guid, 16)), 'of 5000')
print('PINF flag by loaded kind (top):')
for k, c in sorted(kinds.items(), key=lambda x: -sum(x[1].values()))[:30]:
    print('  ', k, dict(c))
pin_unloaded = collections.Counter(f for g, f in pinf.items() if g not in loaded)
print('PINF records not loaded as canonical objects, by flag:', pin_unloaded)
by = collections.defaultdict(collections.Counter)
ex = collections.defaultdict(list)
for s, n in refs.items():
    p = s.split('.')[0]
    g = fold(s)
    st = 'loaded' if g in loaded else ('pinf flag ' + pinf[g] if g in pinf else 'nowhere')
    by[p][st] += 1
    if st != 'loaded' and len(ex[(p, st)]) < 3:
        ex[(p, st)].append(s)
print('referenced FQN strings by class -> where their folded GUID is:')
for p, c in sorted(by.items()):
    print('  ', p, dict(c), {k[1]: v for k, v in ex.items() if k[0] == p})
miss = [l.strip() for l in open(sys.argv[3]) if l.strip()]
c = collections.Counter('loaded' if fold(s) in loaded else ('pinf ' + pinf[fold(s)] if fold(s) in pinf else 'nowhere') for s in miss)
print('volume scripts missing from spice:', len(miss), dict(c))
