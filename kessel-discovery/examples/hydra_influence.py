"""Every Hydra action that changes companion influence, with its handler, contact and amount.
Reads a dump_kind_jsonl Hydra dump. Usage: python3 -I hyd_influence.py <hyd.jsonl>"""
import json, sys, collections

INF = ('Contact Modify Influence', 'Contact Set Influence Level')


def walk(v, handler, out, fqn):
    if isinstance(v, dict):
        if v.get('e5346fa0') in INF:
            out.append((fqn, handler, v['e5346fa0'], v.get('e5346faf'), v.get('90144511'), v.get('ed10e6a4')))
        for k, x in v.items():
            walk(x, handler, out, fqn)
    elif isinstance(v, list):
        for x in v:
            if isinstance(x, dict) and 'k' in x and 'v' in x and isinstance(x['k'], str) and x['k'].startswith('On '):
                walk(x['v'], x['k'], out, fqn)
            else:
                walk(x, handler, out, fqn)


out = []
for line in open(sys.argv[1]):
    if 'Influence' not in line:
        continue
    d = json.loads(line)
    walk(d.get("v", d), None, out, d.get("fqn"))
for r in out:
    print(json.dumps(r))
print(len(out), file=sys.stderr)
