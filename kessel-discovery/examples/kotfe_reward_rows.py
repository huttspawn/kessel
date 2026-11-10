"""KotFE / KotET chapter reward rows from qstRewardsInfoPrototype, with the c788046b flag.

usage: python3 -I kotfe_reward_rows.py <qstRewardsInfoPrototype.json> [quest-substring ...]

qstRewardsInfoPrototype.json: dump_singleton output. Walks every reward row (a dict carrying
d9f35ee5-family keys) under each quest key and prints, per quest, the item and the row's flags:
d9f35ee5 (always granted), c7880469 (difficulty), c788046b (the open flag).
Default quest filter: qst.exp.seasons.0.
"""
import json, sys, collections

d = json.load(open(sys.argv[1]))
want = sys.argv[2:] or ['qst.exp.seasons.0']
rows = collections.defaultdict(list)


def walk(x, quest):
    if isinstance(x, dict):
        q = x.get('k') if isinstance(x.get('k'), str) and x['k'].startswith('@qst.') else quest
        if any(k.startswith('d9f35ee') for k in x) and q:
            item = next((v for v in x.values() if isinstance(v, str) and v.startswith('@itm.')), None)
            rows[q].append((item, x.get('d9f35ee5'), x.get('c7880469'), x.get('c788046b')))
        for v in x.values():
            walk(v, q)
    elif isinstance(x, list):
        for v in x:
            walk(v, quest)


walk(d, None)
for q in sorted(rows):
    if not any(w in q for w in want):
        continue
    L = rows[q]
    flagged = [r for r in L if r[3]]
    weap = [r for r in L if r[0] and ('mainhand' in r[0] or 'offhand' in r[0] or 'weapon' in r[0])]
    print(q, 'rows', len(L), 'flagged', len(flagged), 'weapon rows', len(weap), 'flagged weapons', sum(1 for r in weap if r[3]))
    for r in (flagged or weap)[:3]:
        print('    ', r)
