"""Name every operand of ops 3-7 (conversation conditions and quest complex variables) and
collect sibling comparisons of one (lhs, rhs) pair inside one conversation."""
import json, collections
S = '/home/sean/.claude/tmp/claude-1000/-home-sean-projects-huttspawn-kessel/1753a034-d040-4309-83e8-0c4f3c34307a/scratchpad/'
var = {}; prefix = {}
LIST_TYPE = {}
def walk(v, q, lst=None):
    if isinstance(v, dict):
        if 'b21e56a4' in v and '2adec3c7' in v:
            var.setdefault(v['b21e56a4'] % (1 << 64), (q, v['2adec3c7'], v.get('3b399738'), lst))
        for k, x in v.items(): walk(x, q, k if isinstance(x, list) else lst)
    elif isinstance(v, list):
        for x in v: walk(x, q, lst)
cplx = []
for line in open(S + 'dec-conditions/qst.jsonl'):
    o = json.loads(line); walk(o['v'], o['fqn'])
    b = o['v'].get('e4ee1382')
    if isinstance(b, int): prefix[(b % (1 << 64)) >> 16] = o['fqn']
    for k, L in o['v'].items():
        if isinstance(L, list):
            for d in L:
                if isinstance(d, dict) and isinstance(d.get('bc126751'), list): cplx.append((o['fqn'], d.get('2adec3c7'), d['bc126751']))
def name(t):
    u = t % (1 << 64)
    if u < 4096: return u
    if u in var:
        q, n, c, l = var[u]; return f'{n}' + (f'(={c})' if c is not None else '') + f'[{l}]'
    q = prefix.get(u >> 16)
    if q: return f'{q.split(".")[-1]}#{u & 0xffff}'
    return f'?{u}'
CMP = (3, 4, 5, 6, 7)
out = collections.defaultdict(list); pairs = collections.defaultdict(lambda: collections.defaultdict(set))
cplx_ops = collections.Counter()
for fqn, n, toks in cplx:
    for i, t in enumerate(toks):
        u = t % (1 << 64)
        if u in CMP or u in (10, 12, 14, 15):
            cplx_ops[u] += 1
            if u in CMP and len(out[('cplx', u)]) < 15: out[('cplx', u)].append((fqn, n, [name(x) for x in toks]))
for line in open(S + 'dec-conversation/nodes.jsonl'):
    o = json.loads(line); toks = o['f'].get('bc126754') or []
    for i, t in enumerate(toks):
        u = t % (1 << 64)
        if u in CMP and i >= 2:
            a, b = toks[i - 2] % (1 << 64), toks[i - 1] % (1 << 64)
            if a < 4096 and b < 4096 and len(toks) < 6: continue
            out[('cnv', u)].append((o['fqn'], o['node'], name(a), name(b), [name(x) for x in toks][:20]))
            pairs[o['fqn']][(a, b)].add(u)
        if u in (11, 12) and i >= 2:
            a, b = toks[i - 2] % (1 << 64), toks[i - 1] % (1 << 64)
            pairs[o['fqn']][(a, b)].add(u)
print('complex-variable ops', cplx_ops)
for k in sorted(out):
    print('==', k, len(out[k]))
    for x in out[k][:15]: print('   ', x)
co = collections.Counter()
for fqn, d in pairs.items():
    for ab, s in d.items():
        if len(s) > 1 and s & set(CMP): co[tuple(sorted(s))] += 1
print('op sets used on one (lhs, rhs) inside one conversation:', co.most_common())
