"""The companion reaction table: key, message text (conversationreactions.stb, id2 832245), influence,
and how many conversation nodes use it. Usage: reaction_table.py reactions.json stb.txt reaction_rows.jsonl"""
import json, sys, collections

stb = {}
for line in open(sys.argv[2]):
    p = line.rstrip('\n').split('|', 3)
    if len(p) == 4:
        stb[(int(p[2]) << 32) | int(p[1])] = p[3]
uses = collections.Counter(json.loads(l)['reaction'] for l in open(sys.argv[3]))
out = []
for e in json.load(open(sys.argv[1]))['70f28f35']:
    v = e['v']
    out.append({'key': e['k'], 'text': stb.get(v.get('70f28f31')), 'influence': v.get('70f28f32', 0),
                'display_change': v.get('4ffc3a18', False), 'nodes': uses[e['k']]})
out.sort(key=lambda r: -r['nodes'])
json.dump(out, open('reaction_table.json', 'w'), indent=1)
for r in out:
    print(r['nodes'], r['influence'], r['display_change'], r['text'])
print('text resolved', sum(1 for r in out if r['text']), 'of', len(out))
