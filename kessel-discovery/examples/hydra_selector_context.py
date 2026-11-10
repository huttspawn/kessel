"""For each unresolved hydra selector value: scripts using it, the action names beside it, and
the script's own variable table names (to see whether it is a stale or renamed variable).
usage: sel_context.py <hyd.jsonl> <hydsel_left.json>"""
import json, sys, collections
left = set(json.load(open(sys.argv[2])))
ctx = collections.defaultdict(lambda: {'scripts': collections.Counter(), 'actions': collections.Counter(), 'events': collections.Counter()})


def walk(x, f, ev, out):
    if isinstance(x, dict):
        act = x.get('e5346fa0')
        for k in ('ed10e6a4', '9756f193'):
            v = x.get(k)
            if v in left:
                c = ctx[v]; c['scripts'][f] += 1; c['actions'][(k, act, str(x.get('e5346faf'))[:40])] += 1; c['events'][ev] += 1
        for k, v in x.items():
            walk(v, f, ev, out)
    elif isinstance(x, list):
        for v in x:
            walk(v, f, ev, out)


tabs = {}
for line in open(sys.argv[1]):
    o = json.loads(line); f = o['fqn']
    tabs[f] = sorted({(e['v'].get('080ce2d2', '') or '').rsplit('.', 1)[-1] for e in o.get('ed10e6a0', []) or []})
    sm = o.get('e389e2e3') or []
    if isinstance(sm, list):
        for e in sm:
            if isinstance(e, dict) and 'k' in e:
                walk(e['v'], f, e['k'], None)
    walk({k: v for k, v in o.items() if k not in ('ed10e6a0', 'e389e2e3')}, f, '-', None)
for v, c in sorted(ctx.items(), key=lambda kv: -sum(kv[1]['scripts'].values())):
    print('=====', v, sum(c['scripts'].values()), 'uses in', len(c['scripts']), 'scripts')
    for s, n in c['scripts'].most_common(3):
        print('   script', s, n, 'vars:', tabs.get(s, [])[:25])
    print('   actions', c['actions'].most_common(4))
    print('   events', c['events'].most_common(4))
