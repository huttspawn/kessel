"""cndLogic proof: clauses that cannot hold together (two different classes, two different
disciplines, two different quest-states of one quest...) never sit under AND; and the XOR / type-137 cases."""
import json, collections
D = '/home/sean/.claude/tmp/claude-1000/-home-sean-projects-huttspawn-kessel/1753a034-d040-4309-83e8-0c4f3c34307a/scratchpad/dec-conditions/'
enums = json.load(open('/home/sean/projects/huttspawn/kessel/.claude/worktrees/agent-ab8be80fbd53ff57a/kessel/resources/gom_enums.json'))
CT = next(e for e in enums if e['hash'] == '7C7B7B6804EE5C1F')['members']
def logic(v):
    x = v.get('9e352b11'); return x['enum'] if isinstance(x, dict) else 0
def ctype(c):
    x = c.get('04ee5c20'); return x['enum'] if isinstance(x, dict) else 0
cnt = collections.Counter(); excl = collections.Counter(); examples = collections.defaultdict(list)
types_by_logic = collections.defaultdict(collections.Counter)
xor = []; t137 = []; single = collections.Counter()
for line in open(D + 'cnd.jsonl'):
    o = json.loads(line); v = o['v']; L = logic(v); cl = v.get('04ee5c1e') or []
    cnt[L] += 1
    if len(cl) == 1: single[L] += 1
    for c in cl:
        types_by_logic[L][ctype(c)] += 1
        if ctype(c) == 137: t137.append((o['fqn'], {k: x for k, x in c.items() if x not in ([], None)}, L, len(cl)))
    if L == 2: xor.append((o['fqn'], [(CT[ctype(c)] if ctype(c) < len(CT) else ctype(c), c.get('9d89a8d0'), {k: x for k, x in c.items() if x not in ([], None) and k not in ('04ee5c20',)}) for c in cl]))
    # exclusivity: positive clauses of the same single-valued type with different values
    pos = [c for c in cl if not c.get('9d89a8d0')]
    by = collections.defaultdict(set)
    for c in pos:
        t = ctype(c)
        if t == 3:   # GlobalVariable: same quest base, different state n, both true
            g = c.get('0c0f2895')
            if isinstance(g, int) and c.get('0c0f2896', True) is not False:
                by[('quest', (g % (1 << 64)) >> 32)].add((g % (1 << 64)) & 0xffffffff)
        elif CT[t] if t < len(CT) else False:
            name = CT[t]
            if name in ('cndConditionType_IsClass', 'cndConditionType_IsDiscipline', 'cndConditionType_IsSpecies', 'cndConditionType_IsAdvancedClass', 'cndConditionType_IsFaction', 'cndConditionType_IsGender'):
                key = json.dumps({k: x for k, x in c.items() if x not in ([], None) and k != '04ee5c20'}, sort_keys=True)
                by[name].add(key)
    for k, s in by.items():
        if len(s) > 1 and (k[0] != 'quest' if isinstance(k, tuple) else True):
            excl[(L, k if isinstance(k, str) else k[0])] += 1
            if len(examples[(L, k if isinstance(k, str) else k[0])]) < 3: examples[(L, k if isinstance(k, str) else k[0])].append(o['fqn'])
print('logic counts', cnt, 'single-clause', single)
print('mutually exclusive positive clauses by logic', excl)
for k, e in examples.items(): print(' ', k, e)
print('XOR', len(xor))
for x in xor: print(json.dumps(x)[:900])
print('type 137', t137)
for L in (0, 1):
    print(L, [(CT[t] if t < len(CT) else t, n) for t, n in types_by_logic[L].most_common(8)])
