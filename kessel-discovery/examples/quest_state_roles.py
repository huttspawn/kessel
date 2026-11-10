"""What a conversation does after a gate reads quest state n: does the gated node, or a node below it,
end the quest (cnvNodeQuestEnds), progress it (cnvNodeQuestProgress) or grant it (cnvNodeQuestGrants)?

usage: python3 -I quest_state_roles.py <qst.jsonl> <nodes.jsonl> [n ...]   (default n = 1 2 3 4 5)

Only reads of the form [Q#n, 1, 11] (state true) are counted. Descendants follow cnvNodeChildren
24c978b9 inside one conversation (depth <= 6). Also prints, per n, the other states of the same quest
read in the same gate.
"""
import json, sys, collections

QST, NODES = sys.argv[1], sys.argv[2]
NS = [int(x) for x in sys.argv[3:]] or [1, 2, 3, 4, 5]
M = (1 << 64) - 1
prefix = {}
for line in open(QST):
    o = json.loads(line); b = o['v'].get('e4ee1382')
    if isinstance(b, int):
        prefix[(b & M) >> 16] = o['fqn']


def qset(f, fid):
    return {e['k'][6:] for e in f.get(fid) or [] if isinstance(e, dict) and str(e.get('k', '')).startswith('Quest:')}


convs = collections.defaultdict(dict)
for line in open(NODES):
    o = json.loads(line); f = o['f']
    convs[o['fqn']][o['node']] = (f.get('24c978b9') or [], f.get('bc126754') or [],
                                  qset(f, 'c45852d0'), qset(f, 'c45852d1'), qset(f, 'c45852d2'))
role = {n: collections.Counter() for n in NS}; co = {n: collections.Counter() for n in NS}; ex = collections.defaultdict(list)
for cfqn, nodes in convs.items():
    for nid, (kids, g, gr, en, pr) in nodes.items():
        for i, t in enumerate(g):
            u = t & M
            if u < 4096 or (u >> 16) not in prefix or (u & 0xFFFF) not in NS:
                continue
            if not (i + 2 < len(g) and (g[i + 1] & M) == 1 and (g[i + 2] & M) == 11):
                continue
            n = u & 0xFFFF; q = prefix[u >> 16]
            for j, t2 in enumerate(g):
                u2 = t2 & M
                if j != i and u2 >= 4096 and (u2 >> 16) == (u >> 16) and (u2 & 0xFFFF) < 16 and j + 2 < len(g):
                    co[n][((u2 & 0xFFFF), g[j + 1] & M)] += 1
            seen = {nid}; frontier = [nid]; found = set()
            for depth in range(7):
                nxt = []
                for x in frontier:
                    k, _, gr2, en2, pr2 = nodes.get(x, ([], [], set(), set(), set()))
                    if q in en2: found.add('ends')
                    if q in pr2: found.add('progress')
                    if q in gr2: found.add('grants')
                    for c in k:
                        if c not in seen:
                            seen.add(c); nxt.append(c)
                frontier = nxt
            key = '+'.join(sorted(found)) or 'none'
            role[n][key] += 1
            if len(ex[(n, key)]) < 3:
                ex[(n, key)].append((cfqn, nid, q))
for n in NS:
    tot = sum(role[n].values())
    print(f'n={n}: {tot} true-reads; below the gate: {role[n].most_common()}')
    print(f'      same-gate states of the same quest (n, literal): {co[n].most_common(6)}')
    for k in role[n]:
        for e in ex[(n, k)][:2]:
            print('        ', k, e)
