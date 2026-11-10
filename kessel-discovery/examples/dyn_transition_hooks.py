"""Dyn state transitions (a5db4db3: from 4db0, to 4db1, hook 4db4) against the attached Hydra
script (4fa14a22) and usable states (1e8c7680): is the hook a handler of that script, does the
handler set the dyn's state to the transition's end, and is the start state usable?
usage: dyn_transitions.py <u_Dynamic.jsonl> <hyd.jsonl>"""
import json, sys, collections
handlers = {}
for line in open(sys.argv[2]):
    o = json.loads(line)
    hs = {}
    for e in o.get('e389e2e3') or []:
        out = []

        def acts(x):
            if isinstance(x, dict):
                if 'e5346fa0' in x:
                    out.append((x['e5346fa0'], x.get('e5346faf')))
                for v in x.values():
                    acts(v)
            elif isinstance(x, list):
                for v in x:
                    acts(v)
        acts(e.get('v'))
        hs[e.get('k')] = out
    handlers[o['fqn']] = hs
c = collections.Counter()
for line in open(sys.argv[1]):
    o = json.loads(line); v = o['v']
    tr = v.get('a5db4db3') or []
    if not tr:
        continue
    scr = v.get('4fa14a22')
    scr = scr[1:] if isinstance(scr, str) and scr.startswith('@') else None
    usable = {e['k'] for e in v.get('1e8c7680') or [] if e.get('v') is True}
    for t in tr:
        hook = t.get('a5db4db4'); a, b = t.get('a5db4db0'), t.get('a5db4db1')
        c['transitions'] += 1
        if not hook:
            c['no hook'] += 1; continue
        if not scr:
            c['no script'] += 1; continue
        hs = handlers.get(scr)
        if hs is None:
            c['script not in dump'] += 1; continue
        c['with script'] += 1
        if hook in hs:
            c['hook is a handler of the script'] += 1
            sets = [x for n, x in hs[hook] if n == 'Set Dynamic Placeable State']
            if b in sets:
                c['handler sets the end state'] += 1
            if a in usable:
                c['handler + start state usable'] += 1
        else:
            c['hook not a handler'] += 1
            if a in usable:
                c['not handler + start usable'] += 1
for k, n in c.items():
    print(n, k)
