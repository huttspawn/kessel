"""Show a quest's incoming and outgoing spine edges (all kinds) and its entry points.

usage: python3 -I story_spine_show.py <outdir> <fqn-or-prefix>...
A trailing '*' on an argument matches every quest under that prefix.
"""
import collections
import sys

out = sys.argv[1]
name = {}
for line in list(open(f'{out}/quests.tsv'))[1:]:
    f, n, m, fa = line.rstrip('\n').split('\t')
    name[f] = f'{n or "?"} ({m}{"/" + fa if fa else ""})'
inc, outg = collections.defaultdict(list), collections.defaultdict(list)
for line in list(open(f'{out}/edges.tsv'))[1:]:
    a, b, k, via = line.rstrip('\n').split('\t')
    outg[a].append((k, b, via))
    inc[b].append((k, a, via))
ent = collections.defaultdict(list)
for line in list(open(f'{out}/entries.tsv'))[1:]:
    q, sk, src, via = line.rstrip('\n').split('\t')
    ent[q].append((sk, src, via))
qs = []
for arg in sys.argv[2:]:
    qs += sorted(q for q in name if q.startswith(arg[:-1])) if arg.endswith('*') else [arg]
for q in qs:
    print(f'== {name.get(q)} [{q}]')
    for k, a, via in sorted(inc[q]):
        print(f'   <- {k:8s} {name.get(a)} [{a}]  {via[:110]}')
    for k, b, via in sorted(outg[q]):
        print(f'   -> {k:8s} {name.get(b)} [{b}]  {via[:110]}')
    for sk, src, via in ent[q]:
        print(f'   entry {sk} {src} {via[:100]}')
