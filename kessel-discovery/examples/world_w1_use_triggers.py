"""W1: what the placeables with an empty placeable_triggers row set trigger.
usage: w1.py <spice> <Spawn.jsonl> <measure.tsv>"""
import sqlite3, sys, json, math, collections, ast

db = sqlite3.connect('file:' + sys.argv[1] + '?mode=ro', uri=True)
slot = {}  # (spawner, entity) -> script on that slot (4fa14a23)
for l in open(sys.argv[2]):
    o = json.loads(l)
    for e in o['v'].get('aa610ec0', []) or []:
        if isinstance(e, dict) and '4fa14a23' in e:
            ent = e.get('b78d6eec') or str(e.get('731cd8ba', '')).lstrip('@')
            slot[(o['fqn'], ent)] = str(e['4fa14a23']).lstrip('@')

res = collections.Counter()
per = collections.defaultdict(collections.Counter)
ex = {}
acts_all = collections.Counter()
detail = collections.Counter()
for l in open(sys.argv[3]):
    f = l.rstrip('\n').split('\t')
    if f[0] != 'USE' or f[-1] != 'NO-TRIGGER':
        continue
    p = ast.literal_eval(f[5])
    fqn = f[7]
    rows = db.execute(
        "select l.area_dir_id,l.instance_id,l.log_x,l.log_y,l.log_z,l.script_fqn,p.spawner_fqn "
        "from log_use_targets l join placements p on p.area_dir_id=l.area_dir_id and p.instance_id=l.instance_id "
        "where l.placeable_fqn=?", (fqn,)).fetchall()
    best = min(rows, key=lambda r: math.dist(p, r[2:5]))
    plc_script, spn = best[5], best[6]
    sscript = slot.get((spn, fqn))
    scripts = [s for s in (plc_script, sscript) if s]
    acts = set()
    for s in scripts:
        for h, a, t, v in db.execute(
                "select handler,action,target_fqn,value from script_actions where script_fqn=?", (s,)):
            if h == 'On Use':
                acts.add((a, t or v))
    if not scripts:
        key = 'no-script'
    else:
        key = ('plc' if plc_script else '') + ('+' if plc_script and sscript else '') + ('slot' if sscript else '')
        if not acts:
            key += ':no-On-Use'
    res[key] += 1
    for a, t in acts:
        acts_all[a] += 1
        if a in ('Use Ability', 'Execute Hydra Script', 'Take Taxi Route', 'Teleport To', 'Start Raid Event',
                 'Set Dynamic Placeable State', 'Discover Bindpoint', 'Execute Ability', 'Area Cast'):
            detail[(a, t)] += 1
    per[key][fqn] += 1
    ex.setdefault((key, fqn), (spn, scripts))
print(res)
print(acts_all.most_common(50))
for k, n in detail.most_common(40):
    print('  ', n, k)
for k, c in per.items():
    if k.startswith('no-script') or 'no-On-Use' in k:
        for fqn, n in c.most_common():
            print(k, n, fqn, ex.get((k, fqn)))

# --- categories and Execute Hydra Script chains -------------------------
WHITE = {'Grant Codex', 'Achievement Event', 'Grant Title', 'Give Items', 'Give Quest Item', 'Play Conversation',
         'Play Ambush Conversation', 'Set Quest State', 'Set Quest Granted'}


def handler_actions(script, handler, seen):
    if (script, handler) in seen:
        return []
    seen.add((script, handler))
    out = []
    for a, t, v in db.execute("select action,target_fqn,value from script_actions where script_fqn=? and handler=?",
                              (script, handler)):
        out.append((a, t or v, script, handler))
        if a == 'Execute Hydra Script' and v and ':' in v:
            s, h = v.split(':', 1)
            out += handler_actions(s, 'On Custom:' + h, seen)
    return out


def cat(a, t):
    t = t or ''
    if a == 'Use Ability' and 'healing' in t:
        return 'heal'
    if a in ('Take Taxi Route', 'Teleport To', 'Fade to Black') or 'taxi' in t:
        return 'travel'
    if a in ('Set Dynamic Placeable State', 'Transition to State', 'Reset Placeable'):
        return 'placeable-state'
    if a in ('Set Control State', 'Set Usable Warzone Team', 'Register As Warzone Mapnote',
             'Unregister As Warzone Mapnote') or 'pvp' in t:
        return 'warzone'
    if a in ('Set Active', 'Set Attack Wave', 'Start Raid Event', 'Set Active and Owned'):
        return 'encounter'
    if a in WHITE:
        return 'earn:' + a
    if a in ('Use Ability', 'Area Cast', 'Execute Ability'):
        return 'ability'
    if a == 'Discover Bindpoint':
        return 'bindpoint'
    return None


cats = collections.Counter()
chained = collections.Counter()
earn_ex = collections.Counter()
for l in open(sys.argv[3]):
    f = l.rstrip('\n').split('\t')
    if f[0] != 'USE' or f[-1] != 'NO-TRIGGER':
        continue
    p = ast.literal_eval(f[5])
    fqn = f[7]
    rows = db.execute(
        "select l.log_x,l.log_y,l.log_z,l.script_fqn,p.spawner_fqn from log_use_targets l join placements p "
        "on p.area_dir_id=l.area_dir_id and p.instance_id=l.instance_id where l.placeable_fqn=?", (fqn,)).fetchall()
    best = min(rows, key=lambda r: math.dist(p, r[0:3]))
    scripts = [s for s in (best[3], slot.get((best[4], fqn))) if s]
    seen = set()
    acts = []
    for s in scripts:
        acts += handler_actions(s, 'On Use', seen)
    cs = {cat(a, t) for a, t, s, h in acts} - {None}
    for a, t, s, h in acts:
        if h != 'On Use' and a in WHITE:
            chained[a] += 1
            earn_ex[(fqn, a, t)] += 1
    if not scripts:
        cs = {'use-ability-only'}
    elif not cs:
        cs = {'other:' + ','.join(sorted({a for a, *_ in acts}))[:80]}
    for c in cs:
        cats[c] += 1
print('\nCATEGORIES (a use counts once per category it touches)')
for k, n in cats.most_common():
    print(n, k)
print('earnables reached only through Execute Hydra Script chains:', chained)
for k, n in earn_ex.most_common(20):
    print('  ', n, k)
