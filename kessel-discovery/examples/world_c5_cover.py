"""C5: does PINF list anything kessel does not load? And are wpn/pkg refs (as GUIDs) in PINF?
usage: c5_cover.py <unfiltered> <pinf.tsv> <Spawn.jsonl>"""
import collections, json, sqlite3, sys

un = sqlite3.connect('file:' + sys.argv[1] + '?mode=ro', uri=True)
pinf = {int(g, 16): f for g, f in (l.split() for l in open(sys.argv[2]))}
allg = {}
for g, k, fqn, c in un.execute("select guid, kind, fqn, is_canonical from objects"):
    allg.setdefault(int(g, 16), (k, fqn, c))
rest = [g for g in pinf if g not in allg]
print('PINF records', len(pinf), 'matching any object row (canonical or not)', len(pinf) - len(rest), 'unmatched', len(rest))
print('unmatched by flag', collections.Counter(pinf[g] for g in rest))
nonc = collections.Counter(allg[g][0] for g in pinf if g in allg and not allg[g][2])
print('PINF records matching only non-canonical rows by kind', nonc.most_common(5))
# GUID-valued references that resolve to nothing: are they in PINF?
miss = collections.Counter()


def walk(v):
    if isinstance(v, dict):
        for x in v.values():
            walk(x)
    elif isinstance(v, list):
        for x in v:
            walk(x)
    elif isinstance(v, int) and (v & 0xFFFF000000000000) == 0xE000000000000000 and v not in allg:
        miss['in pinf' if v in pinf else 'not in pinf'] += 1


for f in sys.argv[3:]:
    for l in open(f):
        walk(json.loads(l)['v'])
print('unresolved E000 GUID values in', sys.argv[3:], miss)
