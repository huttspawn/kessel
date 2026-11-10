"""Classify every variable operand of qstReqExpression and cnvConditionCompiled.

usage: python3 -I compiled_operand_census.py <qst.jsonl> <nodes.jsonl> [examples_per_class]

qst.jsonl: dump_kind_jsonl Quest over an unfiltered extract ({"fqn", "v"}).
nodes.jsonl: cnv_conditions / cnv_actions node dump ({"fqn", "node", "f"}).

A token >= 4096 is an operand. Classes:
  named      equals a qstVariableFqnId (b21e56a4) defined by some quest
  state      prefix48 is a quest's (qstAlternateEndingVariableID >> 16) and n < 16 (built-in state)
  hole       prefix48 is a quest's, n >= 16 but no variable of any quest has this id
  noquest    prefix48 matches no extracted quest
"""
import json, sys, collections

qst_path, nodes_path = sys.argv[1], sys.argv[2]
NEX = int(sys.argv[3]) if len(sys.argv) > 3 else 6
M = (1 << 64) - 1
var = {}; prefix = {}


def walk(v, q):
    if isinstance(v, dict):
        if 'b21e56a4' in v and isinstance(v['b21e56a4'], int):
            var.setdefault(v['b21e56a4'] & M, (q, v.get('2adec3c7')))
        for x in v.values():
            walk(x, q)
    elif isinstance(v, list):
        for x in v:
            walk(x, q)


req = []
for line in open(qst_path):
    o = json.loads(line)
    walk(o['v'], o['fqn'])
    b = o['v'].get('e4ee1382')
    if isinstance(b, int):
        prefix[(b & M) >> 16] = o['fqn']
    if o['v'].get('bc126755'):
        req.append((o['fqn'], o['v']['bc126755']))


def klass(u):
    if u in var:
        return 'named'
    q = prefix.get(u >> 16)
    if q is None:
        return 'noquest'
    return 'state' if (u & 0xFFFF) < 16 else 'hole'


def census(src, items):
    c = collections.Counter(); ex = collections.defaultdict(list); distinct = collections.defaultdict(set)
    pfx = collections.Counter()
    for where, toks in items:
        for t in toks:
            u = t & M
            if u < 4096:
                continue
            k = klass(u); c[k] += 1; distinct[k].add(u)
            if k == 'noquest':
                pfx[u >> 16] += 1
            if k in ('hole', 'noquest') and len(ex[k]) < NEX:
                ex[k].append((where, u, prefix.get(u >> 16), u & 0xFFFF))
    total = sum(c.values())
    print(f'== {src}: {total} operand tokens')
    for k in ('named', 'state', 'hole', 'noquest'):
        print(f'   {k:8s} {c[k]:8d} tokens, {len(distinct[k])} distinct')
        for e in ex[k]:
            print('      ', e)
    print('   noquest prefixes (prefix48, tokens):', [(hex(p), n) for p, n in pfx.most_common(12)])
    return c, pfx


census('qstReqExpression', req)


def nodes():
    for line in open(nodes_path):
        o = json.loads(line)
        toks = o['f'].get('bc126754') or []
        if len(toks) > 3:
            yield (o['fqn'], o['node']), toks


census('cnvConditionCompiled', nodes())
