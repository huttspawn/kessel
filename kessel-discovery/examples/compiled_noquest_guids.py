"""List the quest GUIDs behind compiled-expression operands whose prefix matches no extracted quest,
and look each GUID up in an extract's objects table (any kind).

usage: python3 -I compiled_noquest_guids.py <qst.jsonl> <nodes.jsonl> <extract.sqlite> [more.sqlite ...]

The prefix48 of a quest variable token (token >> 16) is the low 48 bits of the owning quest's GUID
(E000 + prefix48), as qstAlternateEndingVariableID e4ee1382 shows for every extracted quest.
Prints one line per distinct GUID: guid, token count, where used, and the extracts that hold it.
"""
import json, sys, sqlite3, collections

qst_path, nodes_path, dbs = sys.argv[1], sys.argv[2], sys.argv[3:]
M = (1 << 64) - 1
prefix = set(); named = set(); req = []


def walk(v):
    if isinstance(v, dict):
        if isinstance(v.get('b21e56a4'), int):
            named.add(v['b21e56a4'] & M)
        for x in v.values():
            walk(x)
    elif isinstance(v, list):
        for x in v:
            walk(x)


for line in open(qst_path):
    o = json.loads(line); walk(o['v'])
    b = o['v'].get('e4ee1382')
    if isinstance(b, int):
        prefix.add((b & M) >> 16)
    if o['v'].get('bc126755'):
        req.append(('qstReq', o['fqn'], o['v']['bc126755']))
uses = collections.defaultdict(list)


def scan(src, where, toks):
    for t in toks:
        u = t & M
        if u >= 4096 and u not in named and (u >> 16) not in prefix:
            uses['E000%012X' % (u >> 16)].append((src, where, u & 0xFFFF))


for src, where, toks in req:
    scan(src, where, toks)
for line in open(nodes_path):
    o = json.loads(line); toks = o['f'].get('bc126754') or []
    if len(toks) > 3:
        scan('cnvCond', o['fqn'], toks)
cons = [sqlite3.connect(d) for d in dbs]
held = collections.Counter()
for g, L in sorted(uses.items(), key=lambda x: -len(x[1])):
    found = []
    for d, c in zip(dbs, cons):
        r = c.execute('select kind, fqn from objects where upper(guid)=? or upper(stable_id)=?', (g, g)).fetchone()
        if r:
            found.append((d.rsplit('/', 1)[-1], r))
    held['held' if found else 'absent'] += 1
    print(g, len(L), sorted({(s, w) for s, w, n in L})[:3], sorted({n for s, w, n in L}), found)
print('distinct guids', len(uses), 'tokens', sum(len(v) for v in uses.values()), dict(held))
