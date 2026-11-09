"""Match NPC positions in combat logs to spawner instances from spn_assets area output.

usage: spn_logmatch.py <spice> <area jsonl> <log area id> <log files...>
Log position (a, b, c) = 10 x world (x, z, y).
"""
import json, math, re, sqlite3, sys, collections

spice, jl, log_area = sys.argv[1], sys.argv[2], sys.argv[3]
db = sqlite3.connect('file:' + spice + '?mode=ro', uri=True)
by_string = collections.defaultdict(set)
for fqn, sid in db.execute("select fqn, string_id from objects where kind='Npc'"):
    by_string[sid].add(fqn)

insts = [json.loads(l) for l in open(jl)]
by_entity = collections.defaultdict(list)
for r in insts:
    if r['world'] and r['entities']:
        for e in r['entities']:
            by_entity[e].append(r)

ent = re.compile(r'\[([^@\[\]|][^\[\]|]*?) \{(\d+)\}:(\d+)\|\(([-\d.]+),([-\d.]+),([-\d.]+),([-\d.]+)\)')
area_re = re.compile(r'AreaEntered \{\d+\}: [^{]*\{(\d+)\}')
first = {}
for f in sys.argv[4:]:
    cur = None
    for line in open(f, encoding='cp1252', errors='replace'):
        m = area_re.search(line)
        if m:
            cur = m.group(1)
            continue
        if cur != log_area:
            continue
        for m in ent.finditer(line):
            name, nid, inst = m.group(1), int(m.group(2)), m.group(3)
            a, b, c = (float(m.group(i)) for i in (4, 5, 6))
            if inst not in first:
                first[inst] = (name, nid, a, b, c)

dists = []
nofqn = nospawn = 0
for inst, (name, nid, a, b, c) in first.items():
    fqns = by_string.get(nid >> 32, set())
    if not fqns:
        nofqn += 1
        continue
    cands = [r for fq in fqns for r in by_entity.get(fq, [])]
    if not cands:
        nospawn += 1
        print('NO SPAWN', name, sorted(fqns)[:2])
        continue
    best = min(cands, key=lambda r: math.dist((a, b, c), (r['world'][0] * 10, r['world'][2] * 10, r['world'][1] * 10)))
    d = math.dist((a, b, c), (best['world'][0] * 10, best['world'][2] * 10, best['world'][1] * 10))
    dists.append(d)
    print('%-32s %8.2f log  (%.1f,%.1f,%.1f) <- %s %s depth=%s' % (name[:32], d, a, b, c, best['spawn'], best['room'], best['depth']))
dists.sort()
print('npc instances', len(first), 'no npc object', nofqn, 'no spawn placed', nospawn)
if dists:
    n = len(dists)
    print('matched', n, 'median', dists[n // 2], 'p90', dists[int(n * .9)], 'within 5', sum(d < 5 for d in dists), 'within 20', sum(d < 20 for d in dists))
