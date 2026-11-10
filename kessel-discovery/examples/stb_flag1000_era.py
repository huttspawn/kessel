"""Line-id (id1) range of 0x1000 cnv entries against all cnv entries, and table areas.
usage: bit1000_era.py <stb_root>"""
import os, sys, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from stb_entries import entries
root = os.path.join(sys.argv[1], 'resources/en-us/str/cnv')
ids = []; allids = []; top = collections.Counter()
for dp, dn, fn in os.walk(root):
    for f in fn:
        p = os.path.join(dp, f)
        for e in entries(p):
            if e[2] & 0xff != 0x41:
                continue
            allids.append(e[0])
            if e[2] & 0x1000:
                ids.append(e[0]); top[os.path.relpath(p, root).split('/')[0]] += 1
ids.sort(); allids.sort()
q = lambda a, x: a[int(x * (len(a) - 1))]
print('0x1000 n', len(ids), 'id1 min', ids[0], 'median', q(ids, .5), 'p99', q(ids, .99), 'max', ids[-1])
print('all    n', len(allids), 'id1 min', allids[0], 'median', q(allids, .5), 'p99', q(allids, .99), 'max', allids[-1])
print('share of all lines with id1 above the 0x1000 max:', sum(1 for i in allids if i > ids[-1]) / len(allids))
print(top.most_common())
