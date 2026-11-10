"""For quests read at each built-in state n, how many carry qstCanBeFailed, qstIsRepeatable,
qstIsHiddenQuest, qstAllowAbandonment -- against all quests.

usage: python3 -I quest_state_flags.py <qst.jsonl> <nodes.jsonl>

Reads are quest-state tokens (prefix48 << 16 | n, n < 16) in cnvConditionCompiled (bc126754).
"""
import json, sys, collections

QST, NODES = sys.argv[1], sys.argv[2]
M = (1 << 64) - 1
FLAGS = {'bd035b90': 'reqPrivacy', '2b8c1d8c': 'canBeFailed', '2c984a81': 'isRepeatable', 'bdd6f0f3': 'isHidden', '9575bff1': 'allowAbandon', '2b8c1d8e': 'failTime', '96ef4d6a1'[-8:]: 'failObjects'}
prefix = {}; flags = {}
for line in open(QST):
    o = json.loads(line); v = o['v']; b = v.get('e4ee1382')
    if isinstance(b, int):
        prefix[(b & M) >> 16] = o['fqn']
    flags[o['fqn']] = {n for k, n in FLAGS.items() if v.get(k) not in (None, False, 0, [], {})}
readq = collections.defaultdict(set)
for line in open(NODES):
    o = json.loads(line)
    for t in o['f'].get('bc126754') or []:
        u = t & M
        if u >= 4096 and (u >> 16) in prefix and (u & 0xFFFF) < 16:
            readq[u & 0xFFFF].add(prefix[u >> 16])


def rate(qs):
    c = collections.Counter(f for q in qs for f in flags.get(q, ()))
    return {f: f'{c[f]}/{len(qs)}' for f in FLAGS.values()}


print('all quests', rate(list(flags)))
for n in sorted(readq):
    print(f'n={n}', rate(readq[n]))
