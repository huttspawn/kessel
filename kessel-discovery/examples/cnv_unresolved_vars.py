"""The action-expression variables that resolve to no quest variable: where do they point?"""
import json, collections
S = '/home/sean/.claude/tmp/claude-1000/-home-sean-projects-huttspawn-kessel/1753a034-d040-4309-83e8-0c4f3c34307a/scratchpad/'
var = {}; prefix = {}; qfqn = set(); qprefix_of = {}; maxn = collections.defaultdict(int)
def walk(v, q):
    if isinstance(v, dict):
        if 'b21e56a4' in v: var.setdefault(v['b21e56a4'] % (1 << 64), (q, v.get('2adec3c7')))
        for x in v.values(): walk(x, q)
    elif isinstance(v, list):
        for x in v: walk(x, q)
for line in open(S + 'dec-conditions/qst.jsonl'):
    o = json.loads(line); qfqn.add(o['fqn']); walk(o['v'], o['fqn'])
    b = o['v'].get('e4ee1382')
    if isinstance(b, int): prefix[(b % (1 << 64)) >> 16] = o['fqn']; qprefix_of[o['fqn']] = (b % (1 << 64)) >> 16
for u, (q, n) in var.items(): maxn[u >> 16] = max(maxn[u >> 16], u & 0xffff)
C = collections.Counter(); rows = []
for line in open(S + 'dec-conversation/nodes.jsonl'):
    o = json.loads(line); f = o['f']; e = f.get('bc126752') or []
    if not e: continue
    a, b, op = e; ua = a % (1 << 64)
    if ua == 0 or ua in var: continue
    q = f.get('b21f4106'); qn = q[6:] if isinstance(q, str) and q.startswith('Quest:') else q
    hook = '0336685a' in f
    p = prefix.get(ua >> 16)
    if p is None: kind = 'prefix of no extracted quest'
    elif p == qn: kind = 'prefix = named quest, n=%s not defined (max defined %s)' % ('builtin' if (ua & 0xffff) < 8 else 'var', maxn[ua >> 16])
    else: kind = 'prefix of another quest'
    C[(hook, kind, qn in qfqn if qn else None)] += 1
    rows.append((o['fqn'], o['node'], qn, hook, kind, p, ua & 0xffff, b, op))
print(C.most_common())
for r in rows[:40]: print(r)
json.dump(rows, open(S + 'dec-logic/unres_rows.json', 'w'))
