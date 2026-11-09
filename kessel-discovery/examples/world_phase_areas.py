"""What do phs area-like ids (7f9e4ac0, 24ed04cc/cd, fd8241fa/fb) point at?
usage: phs_area.py <phs.jsonl> <areas_dir> <missing18> <spice>"""
import collections, json, os, bisect, sqlite3, sys, re

dirs = sorted(int(d) for d in os.listdir(sys.argv[2]))
missing = {int(x) for x in open(sys.argv[3]).read().split()}
db = sqlite3.connect('file:' + sys.argv[4] + '?mode=ro', uri=True)
# ids known from the area.dat files themselves: rooms? read area.dat header ids
vals = collections.defaultdict(list)
for l in open(sys.argv[1]):
    o = json.loads(l)
    for k in ('7f9e4ac0', '24ed04cc', '24ed04cd', 'fd8241fa', 'fd8241fb'):
        if isinstance(o['v'].get(k), int):
            vals[k].append((o['v'][k], o['fqn']))
for k, vs in vals.items():
    d = collections.Counter()
    near_missing = []
    for v, f in vs:
        i = bisect.bisect_left(dirs, v)
        best = min((abs(v - dirs[j]), dirs[j]) for j in (i - 1, i) if 0 <= j < len(dirs))
        d['exact' if best[0] == 0 else ('<=10' if best[0] <= 10 else 'far')] += 1
        if best[0] <= 10 and best[1] in missing:
            near_missing.append((v, best[1], f))
    print(k, len(vs), d, 'distinct', len({v for v, _ in vs}))
    print('   near missing18:', near_missing[:8])
# do the 7f9e4ac0 values appear as text anywhere in area.dat files?
targets = {v for v, _ in vals['7f9e4ac0']}
found = collections.Counter()
for dname in os.listdir(sys.argv[2]):
    b = open(os.path.join(sys.argv[2], dname, 'area.dat'), 'rb').read()
    import struct
    for v in list(targets)[:4000]:
        if struct.pack('<Q', v) in b:
            found[dname] += 1
print('area.dat files containing a phs 7f9e4ac0 id (u64 LE):', len(found), sum(found.values()))
print(' sample', found.most_common(5))
