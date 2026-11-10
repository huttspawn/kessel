"""Print the 'bowdaar influence' handler of the arena controller and the influence-check script, with their branch structure."""
import json, sys
S = '/home/sean/.claude/tmp/claude-1000/-home-sean-projects-huttspawn-kessel/1753a034-d040-4309-83e8-0c4f3c34307a/scratchpad/hyd.jsonl'
want = sys.argv[1:]
for line in open(S):
    o = json.loads(line)
    f = o.get('fqn') or o.get('_fqn') or ''
    if f not in want:
        continue
    for h in o.get('v', o).get('e389e2e3', []):
        if 'influence' in h['k'].lower():
            print('##', f, h['k'])
            print(json.dumps(h['v'], indent=1)[:12000])
