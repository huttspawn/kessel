"""Quest-state codes n in compiled conditions: for nodes that grant / end / progress quest Q,
which state codes of Q does the node's own condition read, and with what literal and op.

usage: python3 -I quest_state_codes.py <qst.jsonl> <nodes.jsonl>
"""
import json, collections, sys
QST, NODES = sys.argv[1], sys.argv[2]
prefix = {}
for line in open(QST):
    o = json.loads(line); b = o['v'].get('e4ee1382')
    if isinstance(b, int): prefix[(b % (1 << 64)) >> 16] = o['fqn']
def qref(t):
    u = t % (1 << 64)
    q = prefix.get(u >> 16)
    return (q, u & 0xffff) if q else None
role_read = collections.Counter()      # (role, n, literal, op)
any_read = collections.Counter()
ops_seen = collections.Counter()
for line in open(NODES):
    o = json.loads(line); f = o['f']; toks = f.get('bc126754') or []
    if len(toks) <= 3: continue
    roles = {}
    for fid, role in (('c45852d0', 'grants'), ('c45852d1', 'ends'), ('c45852d2', 'progress')):
        for e in f.get(fid) or []:
            k = e.get('k') if isinstance(e, dict) else None
            if isinstance(k, str) and k.startswith('Quest:'): roles.setdefault(k[6:], []).append(role)
    for i, t in enumerate(toks):
        r = qref(t) if (t % (1 << 64)) >= 4096 else None
        if not r: continue
        q, n = r
        if n > 16: continue
        nxt = [x % (1 << 64) for x in toks[i + 1:i + 3]]
        if len(nxt) == 2 and nxt[0] < 4096 and nxt[1] in (11, 12) or (len(nxt) == 2 and nxt[1] in (3, 4, 5, 6, 7)):
            lit, op = nxt[0], nxt[1]
        else:
            lit, op = None, 'bare'
        any_read[(n, lit, op)] += 1
        for role in roles.get(q, ['other-quest']):
            role_read[(role, n, lit, op)] += 1
print('state reads n<=16 (n, literal, op):', any_read.most_common(30))
for role in ('grants', 'ends', 'progress', 'other-quest'):
    print(role, [(k[1:], v) for k, v in role_read.most_common() if k[0] == role][:14])
