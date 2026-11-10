"""Read qstErasPrototype: the story eras, their order, gating conditions and ordered quest lists.

usage: python3 -I quest_eras_map.py <qstErasPrototype.json> [qst.jsonl]

qstErasPrototype.json: dump_singleton output. Fields (names from names.json):
  9584dda1 qstErasOrderMap   era order 1..9 -> list of era spec ids (one per class or faction)
  9584dda3 qstErasMap        era spec id -> qstEra
    9584dda4 qstEraOrder, 4a017460 qstEraRequirementsConditionalId, 9584dda9 qstEraVisibilityConditionalId,
    34787442 qstEraQuestOrder (ordered quest list), 49287981 qstEraDeepLinkIds, 96f3d6f0 qstEraImage
  34787445 qstErasIdentifiers, 49602b50 qstEraHighestOrder
With qst.jsonl, also counts how many listed quests exist in the dump.
"""
import json, sys, collections

d = json.load(open(sys.argv[1]))
have = None
if len(sys.argv) > 2:
    have = {json.loads(l)['fqn'] for l in open(sys.argv[2])}
order = {g: (e['k'], i) for e in d['9584dda1'] for i, g in enumerate(e['v'])}
print('identifiers', d.get('34787445')); print('highest order', d.get('49602b50'))
tot = 0; qs = set(); conds = collections.Counter()
rows = []
for e in d['9584dda3']:
    v = e['v']
    q = [x[0] if isinstance(x, list) and x else x for x in v.get('34787442', [])]
    tot += len(q); qs |= {str(x) for x in q}
    conds[v.get('4a017460')] += 1
    rows.append((v.get('9584dda4'), order.get(e['k']), len(q), v.get('4a017460'), v.get('9584dda9'), v.get('96f3d6f0'), q[:1], q[-1:]))
for r in sorted(rows, key=lambda r: (r[0] or 0, str(r[1]))):
    print(r)
print('eras', len(rows), 'quest slots', tot, 'distinct quests', len(qs))
print('requirement conditions', dict(conds))
if have is not None:
    print('listed quests present in qst.jsonl', sum(1 for x in qs if x.lstrip('@') in have), 'of', len(qs))
