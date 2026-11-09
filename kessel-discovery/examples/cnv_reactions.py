"""Companion reactions on conversation nodes (cnvNpcReactions 6c54c424: {6c54c421 companion,
6c54c422 reaction key}) joined to cnvReactionsDataPrototype (70f28f35: key -> 70f28f31 string id,
70f28f32 influence, 4ffc3a18 display change). Usage: reactions.py nodes.jsonl reactions.json"""
import json, sys, collections

table = {e['k']: e['v'] for e in json.load(open(sys.argv[2]))['70f28f35']}
C = collections.Counter()
amounts = collections.Counter()
comp = collections.Counter()
keys = collections.Counter()
rows = open('reaction_rows.jsonl', 'w')
for line in open(sys.argv[1]):
    d = json.loads(line)
    for r in d['f'].get('6c54c424') or []:
        C['reactions'] += 1
        k = r.get('6c54c422')
        keys[k] += 1
        t = table.get(k)
        comp[r.get('6c54c421')] += 1
        if t is None:
            C['key not in table'] += 1
            continue
        C['key in table'] += 1
        amt = t.get('70f28f32', 0)
        amounts[amt] += 1
        rows.write(json.dumps({'cnv': d['fqn'], 'node': d['node'], 'companion': r.get('6c54c421'),
                               'reaction': k, 'influence': amt, 'string_id': t.get('70f28f31')}) + '\n')
print(dict(C))
print('table size', len(table), 'keys used', len(keys), 'used keys in table', sum(1 for k in keys if k in table))
print('amounts', sorted(amounts.items()))
print('companions', len(comp), comp.most_common(8))
print('table amounts', sorted(collections.Counter(v.get('70f28f32', 0) for v in table.values()).items()))
print('unknown keys', [(k, n) for k, n in keys.most_common() if k not in table][:10])
