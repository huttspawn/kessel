"""A9: match bare volume region names to object FQNs: '<prefix>.<name>' exactly, by kind.
Also fold: GOM GUID = 48-bit FNV-1a fold of uppercased FQN (reflection 01a1c265) is tried for unmatched names
under candidate prefixes, against object guids.
usage: a9_match.py <spice> <unfiltered>"""
import collections, sqlite3, sys

db = sqlite3.connect('file:' + sys.argv[1] + '?mode=ro', uri=True)
un = sqlite3.connect('file:' + sys.argv[2] + '?mode=ro', uri=True)
vols = collections.Counter(r for (r,) in db.execute(
    "select region_name from trigger_volumes where region_name is not null and script_fqn is null"))
by_rest = collections.defaultdict(set)
for fqn, kind in un.execute("select distinct fqn, kind from objects"):
    if '.' in fqn:
        p, rest = fqn.split('.', 1)
        by_rest[rest].add(p)
hit = collections.Counter()
hitv = collections.Counter()
unm = collections.Counter()
for n, c in vols.items():
    ps = by_rest.get(n)
    if ps:
        key = 'phs' if 'phs' in ps else ','.join(sorted(ps))
        hit[key] += 1
        hitv[key] += c
    else:
        unm[n] = c
print('distinct bare names', len(vols), 'volumes', sum(vols.values()))
print('matched by <prefix>.<name>:', sum(hit.values()), 'names,', sum(hitv.values()), 'volumes')
for k in hit:
    print('  ', k, hit[k], 'names', hitv[k], 'volumes')
print('unmatched', len(unm), 'names', sum(unm.values()), 'volumes')
print('  top', unm.most_common(40))
print('  first tokens', collections.Counter(n.split('_')[0] for n in unm).most_common(20))
