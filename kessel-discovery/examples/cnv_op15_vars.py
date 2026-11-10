"""For every variable an action writes with op 15, list all writes (op, value) and every gate that reads it.

usage: python3 -I cnv_op15_vars.py <qst.jsonl> <nodes.jsonl>

qst.jsonl: dump_kind_jsonl Quest ({"fqn", "v"}); nodes.jsonl: conversation node dump ({"fqn", "node", "f"}).
Action expressions are cnvActionExpressionCompiled bc126752 = [var, value, op]; gates are
cnvConditionCompiled bc126754 (postfix). Quest complex variables (bc126751) are read too.
Values print as name(=const value) when they are quest constants (qstVariableIdValue 3b399738).
"""
import json, sys, collections

QST, NODES = sys.argv[1], sys.argv[2]
M = (1 << 64) - 1
var = {}
cplx = []


def walk(v, q):
    if isinstance(v, dict):
        if isinstance(v.get('b21e56a4'), int):
            var.setdefault(v['b21e56a4'] & M, (q, v.get('2adec3c7'), v.get('3b399738')))
            if isinstance(v.get('bc126751'), list):
                cplx.append((q, v.get('2adec3c7'), v['bc126751']))
        for x in v.values():
            walk(x, q)
    elif isinstance(v, list):
        for x in v:
            walk(x, q)


for line in open(QST):
    o = json.loads(line)
    walk(o['v'], o['fqn'])


def name(t):
    u = t & M
    if u < 4096:
        return u
    if u in var:
        q, n, c = var[u]
        return str(n) + (f'(={c})' if c is not None else '')
    return f'?{u}'


writes = collections.defaultdict(list); reads = collections.defaultdict(list); nodes = []
for line in open(NODES):
    o = json.loads(line); f = o['f']
    e = f.get('bc126752') or []
    if len(e) == 3:
        writes[e[0] & M].append((o['fqn'], o['node'], e[2] & M, name(e[1])))
    g = f.get('bc126754') or []
    if len(g) > 3:
        nodes.append((o['fqn'], o['node'], g))
targets = {v for v, L in writes.items() if any(op == 15 for _, _, op, _ in L)}
for fqn, node, g in nodes:
    for i, t in enumerate(g):
        if (t & M) in targets and i + 2 < len(g):
            reads[t & M].append((fqn, node, name(g[i + 1]), g[i + 2] & M if (g[i + 2] & M) < 4096 else name(g[i + 2])))
for q, n, g in cplx:
    for i, t in enumerate(g):
        if (t & M) in targets and i + 2 < len(g):
            reads[t & M].append(('cplx:' + q.split('.')[-1], n, name(g[i + 1]), g[i + 2] & M if (g[i + 2] & M) < 4096 else name(g[i + 2])))
OPN = {11: 'set', 12: 'set', 14: '+=', 15: '-=', 3: '!=', 4: '>', 5: '>=', 6: '<', 7: '<=', 8: 'AND?', 9: 'OR?'}
print('variables written with op 15:', len(targets), 'op-15 writes:', sum(op == 15 for L in writes.values() for _, _, op, _ in L))
for v in sorted(targets, key=lambda v: -len(writes[v])):
    q, n, c = var.get(v, ('?', f'?{v}', None))
    w = collections.Counter((OPN.get(op, op), val) for _, _, op, val in writes[v])
    r = collections.Counter((OPN.get(op, op), val) for _, _, val, op in reads[v])
    print(f'{q.split(".")[-1]}.{n}: writes {dict(w)} | reads {dict(r)}')
