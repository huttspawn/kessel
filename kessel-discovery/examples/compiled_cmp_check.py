"""Stack validity with 3-7 as binary comparisons, and the complement pairs (4,7) (5,6) (3,12)."""
import json, collections
S = '/home/sean/.claude/tmp/claude-1000/-home-sean-projects-huttspawn-kessel/1753a034-d040-4309-83e8-0c4f3c34307a/scratchpad/'
BIN = {3, 4, 5, 6, 7, 8, 9, 11, 12}
def valid(toks):
    st = 0
    for t in toks:
        u = t % (1 << 64)
        if u in BIN:
            if st < 2: return False
            st -= 1
        elif u == 10:
            if st < 1: return False
        else: st += 1
    return st == 1
ok = bad = 0; badex = []
pairs = collections.defaultdict(lambda: collections.defaultdict(list))
for line in open(S + 'dec-conversation/nodes.jsonl'):
    o = json.loads(line); toks = o['f'].get('bc126754') or []
    if not toks: continue
    if valid(toks): ok += 1
    else:
        bad += 1
        if len(badex) < 10: badex.append((o['fqn'], o['node'], toks))
    for i, t in enumerate(toks):
        u = t % (1 << 64)
        if u in (3, 4, 5, 6, 7, 11, 12) and i >= 2:
            pairs[o['fqn']][(toks[i - 2], toks[i - 1])].append((u, o['node'], o['f'].get('24c978b9')))
print('stack-valid', ok, 'invalid', bad); [print('  ', b) for b in badex]
for fqn, d in pairs.items():
    for ab, L in d.items():
        s = {x[0] for x in L}
        if s in ({4, 7}, {5, 6}):
            print(fqn, ab, L)
