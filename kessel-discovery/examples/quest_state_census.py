"""Where every built-in quest-state code n (< 16) is read or defined, across the client data.

usage: python3 -I quest_state_census.py <qst.jsonl> <nodes.jsonl> <cnd.jsonl> <extract.sqlite>

Counts reads of quest-state ids in qstReqExpression (bc126755), quest complex variables (bc126751),
conversation gates (bc126754) and actions (bc126752) in the expression form prefix48 << 16 | n, and in
Condition GlobalVariable clauses (0c0f2895) in the form string_id << 32 | n. Also counts quest
variables defined at a slot < 16 (qstVariableFqnId low 16 bits, qstVariableGUID low 32 bits).
"""
import json, sys, sqlite3, collections

QST, NODES, CND, DB = sys.argv[1:5]
M = (1 << 64) - 1
sid = {r[0] for r in sqlite3.connect(DB).execute("select string_id from objects where kind='Quest' and string_id is not null")}
prefix = set(); defined = collections.Counter(); C = collections.Counter()


def walk(v, src):
    if isinstance(v, dict):
        if isinstance(v.get('b21e56a4'), int) and (v['b21e56a4'] & 0xFFFF) < 16:
            defined[('fqnId', v['b21e56a4'] & 0xFFFF)] += 1
        if isinstance(v.get('2c1fe390'), int) and (v['2c1fe390'] & 0xFFFFFFFF) < 16:
            defined[('guid', v['2c1fe390'] & 0xFFFFFFFF)] += 1
        if isinstance(v.get('bc126751'), list):
            src.append(v['bc126751'])
        for x in v.values():
            walk(x, src)
    elif isinstance(v, list):
        for x in v:
            walk(x, src)


cplx = []; req = []
for line in open(QST):
    o = json.loads(line); v = o['v']; b = v.get('e4ee1382')
    if isinstance(b, int):
        prefix.add((b & M) >> 16)
    walk(v, cplx)
    if v.get('bc126755'):
        req.append(v['bc126755'])


def count(src, toks):
    for t in toks:
        u = t & M
        if u >= 4096 and (u >> 16) in prefix and (u & 0xFFFF) < 16:
            C[(src, u & 0xFFFF)] += 1


for t in req:
    count('qstReqExpression', t)
for t in cplx:
    count('qstComplexVariable', t)
for line in open(NODES):
    f = json.loads(line)['f']
    count('cnvCondition', f.get('bc126754') or [])
    count('cnvAction', f.get('bc126752') or [])


def cwalk(v):
    if isinstance(v, dict):
        g = v.get('0c0f2895')
        if isinstance(g, int) and ((g & M) >> 32) in sid and (g & 0xFFFFFFFF) < 16:
            C[('cndGlobalVariable', g & 0xFFFFFFFF)] += 1
        for x in v.values():
            cwalk(x)
    elif isinstance(v, list):
        for x in v:
            cwalk(x)


for line in open(CND):
    cwalk(json.loads(line)['v'])
print('quest variables defined at slot < 16:', dict(defined) or 'none')
for src in ('qstReqExpression', 'qstComplexVariable', 'cndGlobalVariable', 'cnvCondition', 'cnvAction'):
    print(f'{src:20s}', {n: C[(src, n)] for n in range(16) if C[(src, n)]})
