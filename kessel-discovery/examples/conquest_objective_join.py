"""Join kessel conquest objective fields with Exarch's conquest sheet Mission Type."""
import json, sys, sqlite3, collections, re

ach_jsonl, exarch_json, db = sys.argv[1:4]
objs = {}
for line in open(ach_jsonl):
    if '"ach.conquests.' not in line[:60]:
        continue
    o = json.loads(line)
    if not o['fqn'].startswith('ach.conquests.'):
        continue
    objs[o['fqn']] = o['v']

con = sqlite3.connect(db)
names = dict(con.execute("select fqn, name from achievements where fqn like 'ach.conquests.%'"))
descs = dict(con.execute("select fqn, description from achievements where fqn like 'ach.conquests.%'"))

def en(v):
    return v.get('enum') if isinstance(v, dict) else v

rows = {}
for f, v in objs.items():
    rows[f] = dict(
        name=names.get(f), desc=descs.get(f),
        scope=en(v.get('b05565b3')), b1=en(v.get('b05565b1')), e866=en(v.get('e866db30')),
        c4=v.get('4c03d6a1'), cat=v.get('deaaa362'),
        count=(v.get('8ce6ac36') or [{}])[0].get('v', {}).get('deaaa366') if v.get('8ce6ac36') else None,
    )

ex = json.load(open(exarch_json))
extype = collections.defaultdict(collections.Counter)
for sheet in ('Republic Conquest Objectives', 'Imperial Conquest Objectives'):
    for r in ex[sheet][3:]:
        if not r or len(r) < 8 or not r[7]:
            continue
        extype[r[7].strip().lower()][r[6]] += 1

def norm(s):
    return re.sub(r'\s+', ' ', (s or '').strip().lower())

print('exarch types', collections.Counter(t for c in extype.values() for t in c.elements()))
print('kessel scope', collections.Counter(r['scope'] for r in rows.values()))
print('kessel b1', collections.Counter(r['b1'] for r in rows.values()))
print('kessel e866', collections.Counter(r['e866'] for r in rows.values()))
print('kessel c4', collections.Counter(r['c4'] for r in rows.values()))

cross = collections.Counter()
cross_b1 = collections.Counter()
cross_e = collections.Counter()
examples = collections.defaultdict(list)
matched = 0
for f, r in rows.items():
    t = extype.get(norm(r['name']))
    if not t:
        continue
    matched += 1
    top = t.most_common(1)[0][0]
    cross[(r['scope'], top)] += 1
    cross_b1[(r['b1'], top)] += 1
    cross_e[(r['e866'], top)] += 1
    if len(examples[(r['scope'], top)]) < 6:
        examples[(r['scope'], top)].append((f, r['name'], dict(t)))
print('matched', matched, 'of', len(rows))
print('scope x exarch'); [print(' ', k, v) for k, v in sorted(cross.items(), key=str)]
print('b1 x exarch'); [print(' ', k, v) for k, v in sorted(cross_b1.items(), key=str)]
print('e866 x exarch'); [print(' ', k, v) for k, v in sorted(cross_e.items(), key=str)]
for k, v in sorted(examples.items(), key=str):
    print(k)
    for e in v:
        print('   ', e)
json.dump(rows, open(sys.argv[4], 'w'), indent=0)
