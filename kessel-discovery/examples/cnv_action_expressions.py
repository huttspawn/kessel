"""A2 numbers: the conversation actions that name a quest with no hook (cnvActionQuest without
cnvActionHookId), what their cnvActionExpressionCompiled does, and whether its variable is the
named quest's own. Usage: a2.py nodes.jsonl quest.jsonl > a2.txt; writes a2_rows.jsonl."""
import json, sys, collections, re

OPS = {11: 'set bool', 12: 'set int', 14: 'add', 15: 'subtract'}
var = {}
for line in open(sys.argv[2]):
    d = json.loads(line)
    for k, v in d['v'].items():
        if isinstance(v, list):
            for e in v:
                if isinstance(e, dict) and 'b21e56a4' in e:
                    var[e['b21e56a4']] = (d['fqn'], k, e.get('2adec3c7'), e.get('3b399738'))

C = collections.Counter()
other = collections.Counter()
rows = open('a2_rows.jsonl', 'w')
for line in open(sys.argv[1]):
    d = json.loads(line)
    f = d['f']
    e = f.get('bc126752') or []
    q = f.get('b21f4106')
    hook = '0336685a' in f
    if e:
        C['all expressions'] += 1
        a, b, op = e
        if a == 0:
            C['all: lhs 0 (inert)'] += 1
        elif a in var:
            C['all: lhs resolves'] += 1
    if q is None or hook:
        continue
    C['quest, no hook'] += 1
    if not e:
        C['quest, no hook, no expression'] += 1
        for k in f:
            other[k] += 1
        continue
    a, b, op = e
    C[f'op {op} {OPS.get(op)}'] += 1
    va = var.get(a)
    qf = q.split(':', 1)[1] if isinstance(q, str) and ':' in q else None
    if a == 0:
        C['lhs 0 (inert)'] += 1
        continue
    if not va:
        C['lhs unresolved'] += 1
        continue
    C['lhs resolves'] += 1
    if va[0] == qf:
        C['lhs in the named quest'] += 1
    name = va[2] or ''
    if re.match(r'^_b\d+_s\d+_t\d+', name):
        C['lhs is a task flag _bN_sN_tN'] += 1
    vb = var.get(b)
    rhs = {'const': vb[2], 'value': vb[3]} if vb else b
    rows.write(json.dumps({'cnv': d['fqn'], 'node': d['node'], 'quest': qf, 'var': name,
                           'var_list': va[1], 'op': OPS.get(op, op), 'rhs': rhs}) + '\n')
for k, n in sorted(C.items()):
    print(f'{k}: {n}')
print('fields on quest-no-hook-no-expression nodes:', other.most_common(25))
