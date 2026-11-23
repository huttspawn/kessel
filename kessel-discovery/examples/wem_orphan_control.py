"""wem_orphan_control -- are the orphan streamed-wem ids' u32 hits in .dat
files and GOM payloads (wem_orphan_refs) more than chance? Counts the same
hits for N random u32 controls drawn from the orphan id range (seeded, none a
swept wem id). Equal rates mean the hits are coincidental byte matches.

  python3 -I wem_orphan_control.py <wem_ids.tsv> <spice.sqlite> <dat_dir> [seed]
"""
import base64, json, os, random, sqlite3, sys
import numpy as np

ids_f, spice, datd = sys.argv[1:4]
seed = int(sys.argv[4]) if len(sys.argv) > 4 else 429
orph = json.load(open(ids_f + '.orphans.json'))['orphans']
sweep = {int(l.split()[1]) for l in open(ids_f)}
on = sorted(set(orph.values()))
rnd = random.Random(seed); lo, hi = min(on), max(on); ctl = set()
while len(ctl) < len(on):
    x = rnd.randint(lo, hi)
    if x not in sweep: ctl.add(x)
sets = {'orphan': np.array(on, dtype=np.uint32), 'control': np.array(sorted(ctl), dtype=np.uint32)}
R = {k: {'dat_uses': 0, 'dat_ids': set(), 'gom_uses': 0, 'gom_ids': set()} for k in sets}


def scan(body, key, tag):
    for i in range(4):
        n = (len(body) - i) // 4
        if n <= 0: continue
        a = np.frombuffer(body, dtype='<u4', count=n, offset=i)
        for k, ids in sets.items():
            m = a[np.isin(a, ids)]
            R[k][tag + '_uses'] += len(m); R[k][tag + '_ids'].update(int(x) for x in m)


for fn in os.listdir(datd):
    scan(open(os.path.join(datd, fn), 'rb').read(), None, 'dat')
db = sqlite3.connect(f'file:{spice}?mode=ro', uri=True)
for (j,) in db.execute('select json from objects'):
    p = json.loads(j).get('payload_b64')
    if p: scan(base64.b64decode(p), None, 'gom')
print(json.dumps({k: {kk: (len(vv) if isinstance(vv, set) else vv) for kk, vv in v.items()} for k, v in R.items()}, indent=1))
