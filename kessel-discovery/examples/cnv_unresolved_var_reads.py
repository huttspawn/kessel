"""Are the unresolved action variables read anywhere (conversation conditions, quest expressions, complex variables, raw quest payload)?

usage: python3 -I cnv_unresolved_var_reads.py <qst.jsonl> <nodes.jsonl> <unres_rows.json from cnv_unresolved_vars.py>
"""
import json, collections, sys
QST, NODES, ROWS = sys.argv[1], sys.argv[2], sys.argv[3]
prefix = {}
for line in open(QST):
    o = json.loads(line); b = o['v'].get('e4ee1382')
    if isinstance(b, int): prefix[o['fqn']] = (b % (1 << 64)) >> 16
rows = json.load(open(ROWS))
ids = {}
for r in rows:
    if r[5]: ids[(prefix[r[5]] << 16) | r[6]] = r
signed = {(u - (1 << 64) if u >= 1 << 63 else u): u for u in ids}
reads = collections.Counter(); where = collections.defaultdict(set)
for line in open(NODES):
    o = json.loads(line); f = o['f']
    for fid in ('bc126754', 'bc126753'):
        for t in f.get(fid) or []:
            u = t % (1 << 64)
            if u in ids: reads[u] += 1; where[u].add(o['fqn'])
qreads = collections.Counter()
for line in open(S + 'dec-conditions/qst.jsonl'):
    for s, u in signed.items():
        if str(s) in line or str(u) in line:
            qreads[u] += 1
print('distinct unresolved ids', len(ids), 'read by conversation conditions:', len(reads), 'present anywhere in a quest payload:', len(qreads))
for u in list(reads)[:8]: print('  ', ids[u][2], ids[u][6], reads[u], sorted(where[u])[:3])
