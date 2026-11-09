"""Per conversation hook: how many tasks of the named quest carry that hook (unique join?),
using the unfiltered quest dump; unresolved split by hook name.

Run in a directory holding: hooks.tsv (cnv_hook_dump.rs), u_Quest.jsonl (dump_kind_jsonl.rs Quest)
and questguid.tsv (`select guid||char(9)||fqn from objects where kind='Quest' and is_canonical=1`).
"""
import json, collections

P = 0x100000001b3; M = (1 << 64) - 1


def fnv(s):
    h = 0xcbf29ce484222325
    for c in s.encode(): h ^= c; h = (h * P) & M
    return h - (1 << 64) if h >> 63 else h


names = {fnv(n): n for n in ['ON START', 'ON END', 'ON USE', 'ON DEATH', 'ON LOOT', 'ON HYDRA',
                              'ON QUEST COMPLETE', 'ON SURRENDER', 'ON START ABANDONED']}
for i in range(1, 40):
    names[fnv(f'PROGRESS {i}')] = f'PROGRESS {i}'
guid = {}
for line in open('questguid.tsv'):
    g, f = line.rstrip('\n').split('\t'); guid[f] = g.upper()
tasks = {}   # quest guid -> hook -> [(b,s,t,last)]
steps_of = {}
for line in open('u_Quest.jsonl'):
    o = json.loads(line); g = guid.get(o['fqn'])
    d = collections.defaultdict(list)
    for b, br in enumerate(o['v'].get('05a2a6d3', [])):
        sts = br.get('05a2a6d1', [])
        for s, st in enumerate(sts):
            for t, task in enumerate(st.get('05a2a6d0', [])):
                if '64287098' in task:
                    d[task['64287098']].append((b, s, t, s + 1 == len(sts)))
    tasks[g] = d
res = collections.Counter(); multi = collections.Counter(); miss = collections.Counter()
hookname = collections.Counter()
for line in open('hooks.tsv'):
    cnv, node, role, q, h, flags, tags = line.rstrip('\n').split('\t')
    if not h:
        res[(role, 'no hook')] += 1; continue
    h = int(h)
    hookname[names.get(h, '?')] += 1
    d = tasks.get(q)
    if d is None:
        res[(role, 'quest missing')] += 1; continue
    n = len(d.get(h, []))
    if n == 0:
        res[(role, 'no task with hook')] += 1; miss[names.get(h, h)] += 1
    elif n == 1:
        res[(role, 'unique task')] += 1
    else:
        steps = {(b, s) for b, s, t, _ in d[h]}
        res[(role, 'several tasks, one step' if len(steps) == 1 else 'several steps')] += 1
        if len(steps) > 1:
            multi[names.get(h, h)] += 1
for k, c in sorted(res.items()):
    print(c, k)
print('hook names over all conversation hooks:', hookname.most_common(8), 'unnamed', hookname['?'])
print('several-steps by hook', multi.most_common(8))
print('missing by hook', miss.most_common(8))
