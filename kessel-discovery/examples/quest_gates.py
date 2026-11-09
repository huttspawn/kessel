"""Quest -> quest gates from client data (gaps A4/A5).

Two sources:
  qstReqExpression (bc126755): postfix tokens; 8 AND, 9 OR, 10 NOT; any other token is a
    quest variable (prefix48 << 16 | n), prefix48 = owning quest's qstAlternateEndingVariableID
    (e4ee1382) >> 16; n = 1 ACTIVE, 2 COMPLETE, 4 ELIGIBLE, else a named variable whose
    qstVariableFqnId (b21e56a4) equals the token.
  qstReqCondition (3b326b48) -> Condition clauses: GlobalVariable (type 3) on
    (quest string_id << 32 | n), IsOnQuest 117, HasCompletedQuest 118, IsEligibleForQuest 116,
    through NestedConditional 19, with cndNegate and cndGlobalVariableBooleanValue folded in.

Inputs are dump_kind_jsonl output for kinds Quest and Condition.

Usage:
  python3 -I quest_gates.py <Quest.jsonl> <Condition.jsonl> <spice.sqlite> <out.tsv>
"""
import collections
import json
import sqlite3
import sys

BUILTIN = {1: 'ACTIVE', 2: 'COMPLETE', 4: 'ELIGIBLE'}
OPS = {8: 'AND', 9: 'OR', 10: 'NOT'}
VAR_LISTS = ('af9dc3a6', 'e1f06c55')


def main(qst_path, cnd_path, db_path, out_path):
    db = sqlite3.connect(db_path)
    sid_quest = {s: f for f, s in db.execute("select fqn, string_id from objects where kind='Quest'")}
    quests = [json.loads(line) for line in open(qst_path)]
    conds = {}
    for line in open(cnd_path):
        o = json.loads(line)
        conds[o['fqn']] = o['v']
    prefix, varname = {}, {}
    for o in quests:
        v = o['v']
        base = v.get('e4ee1382')
        if isinstance(base, int):
            prefix.setdefault((base % (1 << 64)) >> 16, o['fqn'])
        for k, lst in v.items():
            if not (k.startswith('ad6400') or k in VAR_LISTS) or not isinstance(lst, list):
                continue
            for d in lst:
                if isinstance(d, dict) and 'b21e56a4' in d:
                    varname[d['b21e56a4'] % (1 << 64)] = d.get('2adec3c7')

    def expr_leaves(tokens):
        stack = []
        for t in tokens:
            u = t % (1 << 64) if isinstance(t, int) else t
            if u in OPS:
                n = 1 if u == 10 else 2
                if len(stack) < n:
                    return None
                args = stack[-n:]
                del stack[-n:]
                stack.append((OPS[u], args))
            else:
                stack.append(('VAR', u))
        if len(stack) != 1:
            return None
        out = []

        def walk(node, neg):
            op, a = node
            if op == 'VAR':
                if isinstance(a, int):
                    lo = a & 0xffff
                    out.append((prefix.get(a >> 16, hex(a)), BUILTIN.get(lo) or varname.get(a) or str(lo), neg))
            elif op == 'NOT':
                walk(a[0], not neg)
            else:
                for x in a:
                    walk(x, neg)
        walk(stack[0], False)
        return out

    def cnd_leaves(fqn, neg=False, seen=()):
        v = conds.get(fqn)
        if not v or fqn in seen:
            return
        for c in v.get('04ee5c1e', []):
            t = c.get('04ee5c20', {}).get('enum', 0)
            n = neg ^ bool(c.get('9d89a8d0'))
            if t == 19 and isinstance(c.get('a978c630'), str):
                yield from cnd_leaves(c['a978c630'][1:], n, seen + (fqn,))
            elif t == 3 and isinstance(c.get('0c0f2895'), int):
                x = c['0c0f2895']
                q = sid_quest.get(x >> 32)
                if q:
                    yield q, BUILTIN.get(x & 0xffffffff, str(x & 0xffffffff)), n ^ (not c.get('0c0f2892', False))
            elif t in (116, 117, 118) and str(c.get('e9ae7d13', '')).startswith('@qst.'):
                yield c['e9ae7d13'][1:], {116: 'ELIGIBLE', 117: 'ACTIVE', 118: 'COMPLETE'}[t], n

    stats = collections.Counter()
    with open(out_path, 'w') as out:
        out.write('quest\tsource\tpolarity\ttarget\tvar\n')
        for o in quests:
            v = o['v']
            if v.get('bc126755'):
                leaves = expr_leaves(v['bc126755'])
                if leaves is None:
                    stats['bad expression'] += 1
                    continue
                stats['expressions'] += 1
                for tgt, var, neg in leaves:
                    out.write(f"{o['fqn']}\tqstReqExpression\t{'not' if neg else 'req'}\t{tgt}\t{var}\n")
                    stats['expression edges'] += 1
            if isinstance(v.get('3b326b48'), str):
                stats['conditions'] += 1
                for tgt, var, neg in cnd_leaves(v['3b326b48'][1:]):
                    out.write(f"{o['fqn']}\tqstReqCondition\t{'not' if neg else 'req'}\t{tgt}\t{var}\n")
                    stats['condition edges'] += 1
    print(dict(stats))


if __name__ == '__main__':
    main(*sys.argv[1:5])
