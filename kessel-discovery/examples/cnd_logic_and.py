"""AND side of cndLogic: multi-clause conditions by name word, and level-range conditions.

usage: python3 -I cnd_logic_and.py <cnd.jsonl> [gom_enums.json]  (default: kessel/resources/gom_enums.json in this repo)
"""
import json, collections, re, sys
CND = sys.argv[1]
import os
ENUMS = sys.argv[2] if len(sys.argv) > 2 else os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'kessel', 'resources', 'gom_enums.json')
enums = json.load(open(ENUMS))
CT = next(e for e in enums if e['hash'] == '7C7B7B6804EE5C1F')['members']
def logic(v):
    x = v.get('9e352b11'); return x['enum'] if isinstance(x, dict) else 0
def ctype(c):
    x = c.get('04ee5c20'); return x['enum'] if isinstance(x, dict) else 0
word = collections.Counter(); ex = collections.defaultdict(list)
lvl = collections.Counter(); lvlex = []
for line in open(CND):
    o = json.loads(line); v = o['v']; cl = v.get('04ee5c1e') or []
    if len(cl) < 2: continue
    L = logic(v); toks = set(re.split(r'[._]', o['fqn']))
    for w in ('and', 'or', 'any', 'all', 'either', 'both', 'not'):
        if w in toks:
            word[(w, L)] += 1
            if len(ex[(w, L)]) < 3: ex[(w, L)].append(o['fqn'])
    names = [CT[ctype(c)] if ctype(c) < len(CT) else ctype(c) for c in cl]
    if names.count('cndConditionType_IsLevel') >= 2:
        lvl[L] += 1
        if len(lvlex) < 4: lvlex.append((o['fqn'], L, [{k: x for k, x in c.items() if x not in ([], None)} for c in cl]))
print(sorted(word.items())); [print(' ', k, e) for k, e in sorted(ex.items())]
print('two IsLevel clauses by logic', lvl)
for x in lvlex: print(json.dumps(x)[:500])
