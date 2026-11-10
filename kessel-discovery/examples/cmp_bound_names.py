"""Strict versus non-strict comparisons (ops 4-7) against names that state a bound.

usage: python3 -I cmp_bound_names.py <qst.jsonl> <nodes.jsonl> <cnd.jsonl>

Collects every complex quest variable (name + bc126751 expression) whose name carries a bound word
(at_least, more_than, greater, over, above, under, below, less, fewer, max, min, or_more, or_less,
exceed, reach, enough) and every op 4-7 use, and prints name, op and operands. Also prints
Condition clauses whose cndComparison (enum cndComparison: 0 !=, 1 <, 2 <=, 3 ==, 4 >=, 5 >) sits
under a Condition name with a bound word, as a cross-check of how designers name bounds.
"""
import json, re, sys, collections

QST, NODES, CND = sys.argv[1:4]
M = (1 << 64) - 1
WORDS = re.compile(r'at_least|atleast|more_than|greater|_over_|above|under|below|less|fewer|_max|_min|or_more|or_less|exceed|reach|enough|minimum|maximum|plus', re.I)
OPS = {4: '>', 5: '>=', 6: '<', 7: '<='}
var = {}; cplx = []


def walk(v, q):
    if isinstance(v, dict):
        if isinstance(v.get('b21e56a4'), int):
            var[v['b21e56a4'] & M] = (v.get('2adec3c7'), v.get('3b399738'))
            if isinstance(v.get('bc126751'), list):
                cplx.append((q, v.get('2adec3c7'), v['bc126751']))
        for x in v.values():
            walk(x, q)
    elif isinstance(v, list):
        for x in v:
            walk(x, q)


for line in open(QST):
    o = json.loads(line); walk(o['v'], o['fqn'])


def nm(t):
    u = t & M
    if u < 4096:
        return u
    n, c = var.get(u, (None, None))
    return f'{n}' + (f'(={c})' if c is not None else '') if n else '?'


print('== complex variables with a bound word and an op 4-7')
for q, n, toks in cplx:
    ops = [t & M for t in toks if (t & M) in OPS]
    if ops and n and WORDS.search(n):
        print('  ', q.split('.')[-1], n, [OPS.get(t & M, nm(t)) if (t & M) in OPS else nm(t) for t in toks])
print('== op 4-7 operands whose variable or constant name carries a bound word')
seen = collections.Counter()
for line in open(NODES):
    o = json.loads(line); g = o['f'].get('bc126754') or []
    for i, t in enumerate(g):
        u = t & M
        if u in OPS and i >= 2:
            a, b = nm(g[i - 2]), nm(g[i - 1])
            if WORDS.search(f'{a} {b}'):
                key = (OPS[u], str(a), str(b))
                if seen[key] == 0:
                    print('  ', o['fqn'].split('.')[-1], o['node'], a, OPS[u], b)
                seen[key] += 1
CMP = {0: '!=', 1: '<', 2: '<=', 3: '==', 4: '>=', 5: '>'}
print('== Conditions named with a bound word (cndComparison cross-check)')
k = 0
for line in open(CND):
    o = json.loads(line)
    if not WORDS.search(o['fqn']):
        continue
    for c in o['v'].get('04ee5c1e') or []:
        for f, v in c.items():
            if isinstance(v, dict) and 'enum' in v and f in ('0c0f2894', '6364561a', '362260fb', '0c0f288f', '0c0f288c'):
                if k < 30:
                    print('  ', o['fqn'], CMP.get(v['enum']), {x: y for x, y in c.items() if isinstance(y, (int, float)) and not isinstance(y, bool)})
                k += 1
