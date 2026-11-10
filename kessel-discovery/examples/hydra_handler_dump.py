"""Print the 'bowdaar influence' handler of the arena controller and the influence-check script, with their branch structure.

usage: python3 -I hydra_handler_dump.py <hyd.jsonl> <hydra fqn> [hydra fqn ...]  (prints handlers whose name contains "influence")
"""
import json, sys
S = sys.argv[1]
want = sys.argv[2:]
for line in open(S):
    o = json.loads(line)
    f = o.get('fqn') or o.get('_fqn') or ''
    if f not in want:
        continue
    for h in o.get('v', o).get('e389e2e3', []):
        if 'influence' in h['k'].lower():
            print('##', f, h['k'])
            print(json.dumps(h['v'], indent=1)[:12000])
