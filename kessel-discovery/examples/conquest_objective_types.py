"""Per-event objective type (cnqConquestAchievementTypeEnum) vs achRepeatInterval vs Exarch's Mission Type."""
import json, sys, sqlite3, collections, re

D = sys.argv[1]
db = sys.argv[2]
exarch = sys.argv[3]
info = json.load(open(D + '/cnqConquestInfo.json'))
rows = json.load(open(D + '/cnq_rows.json'))
con = sqlite3.connect(db)

def sname(sid):
    if sid is None:
        return None
    id1 = sid & 0xffffffff
    r = con.execute("select text from strings where fqn like 'str.gui.planetaryconquest%' and id1=? and locale='en-us'", (id1,)).fetchone()
    return r[0] if r else None

CT = {0: 'Invalid', 1: 'Unrepeatable', 2: 'Repeatable', 3: 'DailyRepeatable'}
RI = {None: 'Never(absent)', 0: 'Never', 1: 'Infinitely', 2: 'Daily', 3: 'Weekly'}
events = {}
cross = collections.Counter()
per_obj_types = collections.defaultdict(set)
for x in info['295b3318']:
    v = x['v']
    name = v.get('8c7dafec')
    disp = sname(v.get('8c7dafed'))
    amap = {}
    for e in v.get('8f33cf59') or []:
        k = e['k']
        t = e['v']['enum'] if isinstance(e['v'], dict) else e['v']
        amap[k.lstrip('@') if isinstance(k, str) else k] = t
    events[name] = dict(disp=disp, amap=amap)
    c = collections.Counter(amap.values())
    print(name, '|', disp, '| objectives', len(amap), {CT[k]: n for k, n in sorted(c.items())})
    for a, t in amap.items():
        r = rows.get(a)
        ri = r['scope'] if r else 'missing'
        cross[(CT[t], RI.get(ri, ri))] += 1
        per_obj_types[a].add(t)

print('\nevent-type x achRepeatInterval (objective-in-event pairs)')
for k, n in sorted(cross.items(), key=str):
    print(' ', k, n)
print('objectives whose type differs across events:', sum(1 for s in per_obj_types.values() if len(s) > 1), 'of', len(per_obj_types))
print('distinct objectives in any event:', len(per_obj_types), 'unreferenced conquest achievements:', len(set(rows) - set(per_obj_types)))
unref = collections.Counter(f.split('.')[2] for f in set(rows) - set(per_obj_types))
print('  unreferenced by category', unref.most_common())
unref_scope = collections.Counter(rows[f]['scope'] for f in set(rows) - set(per_obj_types))
ref_scope = collections.Counter(rows[f]['scope'] for f in per_obj_types if f in rows)
print('  scope of unreferenced', unref_scope, 'of referenced', ref_scope)

# Exarch: (event display, mission name) -> type
ex = json.load(open(exarch))
def norm(s):
    return re.sub(r'\s+', ' ', (s or '').strip().lower())
extab = collections.defaultdict(collections.Counter)
for sheet in ('Republic Conquest Objectives', 'Imperial Conquest Objectives'):
    for r in ex[sheet][3:]:
        if not r or len(r) < 8 or not r[7] or not r[4]:
            continue
        extab[(norm(r[4]), norm(r[7]))][r[6]] += 1
names = {}
for f, r in rows.items():
    names.setdefault(norm(r['name']), []).append(f)
xc = collections.Counter()
xri = collections.Counter()
mism = []
for (ev, mn), ty in extab.items():
    evs = [e for e in events.values() if norm(e['disp']) == ev]
    if not evs:
        continue
    e = evs[0]
    cands = [f for f in names.get(mn, []) if f in e['amap']]
    if len(cands) != 1:
        continue
    f = cands[0]
    t = e['amap'][f]
    top = ty.most_common(1)[0][0]
    xc[(CT[t], top)] += 1
    xri[(RI.get(rows[f]['scope']), top)] += 1
    if (CT[t], top) not in (('Unrepeatable', 'Weekly'), ('Repeatable', 'Infinitely Repeatable'), ('DailyRepeatable', 'Daily')):
        mism.append((ev, mn, f, CT[t], RI.get(rows[f]['scope']), dict(ty)))
print('\nevent-type x Exarch (event, objective) pairs')
for k, n in sorted(xc.items(), key=str):
    print(' ', k, n)
print('achRepeatInterval x Exarch on the same pairs')
for k, n in sorted(xri.items(), key=str):
    print(' ', k, n)
print('mismatches', len(mism))
for m in mism[:40]:
    print('  ', m)
json.dump({k: dict(disp=v['disp'], amap=v['amap']) for k, v in events.items()}, open(D + '/events.json', 'w'))
