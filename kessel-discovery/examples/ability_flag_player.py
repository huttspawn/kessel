"""Flag 97d6d791 on player class actives (abl.<class>.<name>, no skill/mods, not passive): best
single-field and two-field predictors, and the full flagged / unflagged name lists per class.
usage: flag_player.py <abl_all.jsonl> <spice.sqlite>"""
import json, sys, sqlite3, collections, itertools
abl, db = sys.argv[1:3]
CLS = ('agent', 'smuggler', 'bounty_hunter', 'trooper', 'sith_warrior', 'jedi_knight', 'sith_inquisitor', 'jedi_consular')
con = sqlite3.connect(db)
canon = {g for (g,) in con.execute("select guid from objects where kind='Ability' and is_canonical=1")}
name = {int(i): t for i, t in con.execute("select id2, text from strings where fqn like 'str.abl.0.%'")}
sid = {f: s for f, s in con.execute("select fqn, string_id from objects where kind='Ability' and is_canonical=1")}
rows = []
for line in open(abl):
    o = json.loads(line)
    f = o['fqn']; p = f.split('.')
    if o.get('guid') not in canon or len(p) != 3 or p[1] not in CLS:
        continue
    v = o['v']
    if v.get('400000003d2e41ef'):
        continue
    feats = set()
    for k, x in v.items():
        if k.endswith('97d6d791'):
            continue
        k8 = k[-8:]
        feats.add(k8)
        if isinstance(x, (bool, int, float, str)) and len(str(x)) < 24:
            feats.add('%s=%s' % (k8, x))
        if isinstance(x, dict) and 'enum' in x:
            feats.add('%s=e%s' % (k8, x['enum']))
    rows.append((bool(v.get('4000000197d6d791')), name.get(int(sid.get(f) or 0), ''), f, feats))
print('player actives', len(rows), 'flagged', sum(r[0] for r in rows))
allf = collections.Counter(x for r in rows for x in r[3])
best = []
for fe, n in allf.items():
    if n < 15:
        continue
    tp = sum(1 for r in rows if r[0] and fe in r[3]); fp = n - tp
    fn = sum(1 for r in rows if r[0] and fe not in r[3]); tn = len(rows) - tp - fp - fn
    acc = max(tp + tn, fp + fn) / len(rows)
    best.append((acc, fe, tp, fp, fn, tn))
best.sort(reverse=True)
for b in best[:20]:
    print('  acc %.3f %-28s flagged&has %d unflagged&has %d flagged&lacks %d unflagged&lacks %d' % b)
for flag in (False, True):
    print('== flagged' if flag else '== unflagged')
    print('   ' + '; '.join(sorted({r[1] for r in rows if r[0] == flag and r[1]})))
