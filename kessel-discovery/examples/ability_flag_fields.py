"""Which ability fields (and scalar values) best predict 97d6d791 on canonical abilities.
usage: flag_fields.py <abl_all.jsonl> <spice.sqlite>"""
import json, sys, sqlite3, collections
abl, db = sys.argv[1:3]
con = sqlite3.connect(db)
canon = {g for (g,) in con.execute("select guid from objects where kind='Ability' and is_canonical=1")}
FLAG = '4000000197d6d791'
nf = collections.Counter(); nb = collections.Counter(); N = 0; F = 0
for line in open(abl):
    o = json.loads(line)
    if o.get('guid') not in canon:
        continue
    v = o['v']; flag = bool(v.get(FLAG)); N += 1; F += flag
    keys = set()
    for k, x in v.items():
        if k == FLAG:
            continue
        keys.add(k)
        if isinstance(x, (bool, int, str)) and len(str(x)) < 30:
            keys.add(k + '=' + str(x))
        if isinstance(x, dict) and 'enum' in x:
            keys.add(k + '=enum' + str(x['enum']))
    for k in keys:
        nb[k] += 1
        if flag:
            nf[k] += 1
print('canonical', N, 'flagged', F)
rows = []
for k, n in nb.items():
    if n < 200:
        continue
    p = nf[k] / n
    rows.append((p, n, k))
rows.sort()
print('lowest flag share (predicts unflagged):')
for r in rows[:15]:
    print('   %.3f %6d %s' % r)
print('highest flag share:')
for r in rows[-15:]:
    print('   %.3f %6d %s' % r)
