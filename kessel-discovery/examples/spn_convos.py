"""For each 'In Conversation' in an area, the NPC from the TargetSet just before it and its spawner distance.
usage: convos.py <spice> <area jsonl> <log area id> <log file>"""
import json, math, re, sqlite3, sys, collections

spice, jl, log_area, f = sys.argv[1:5]
db = sqlite3.connect('file:' + spice + '?mode=ro', uri=True)
by_string = collections.defaultdict(set)
for fqn, sid in db.execute("select fqn, string_id from objects where kind='Npc'"):
    by_string[sid].add(fqn)
rows = [json.loads(l) for l in open(jl)]
rows = [r for r in rows if r['world']]
by_entity = collections.defaultdict(list)
for r in rows:
    for e in r['entities'] or []:
        by_entity[e].append(r)


def lp(r):
    w = r['world']
    return (w[0] * 10, w[2] * 10, w[1] * 10)


def ts(s):
    h, m, x = s.split(':')
    return int(h) * 3600 + int(m) * 60 + float(x)


area_re = re.compile(r'AreaEntered \{\d+\}: [^{]*\{(\d+)\}')
tgt = re.compile(r'^\[([\d:.]+)\] \[@[^\]]*\] \[([^@\[\]|][^\[\]|]*?) \{(\d+)\}:(\d+)\|\(([-\d.]+),([-\d.]+),([-\d.]+),[-\d.]+\)\|[^\]]*\] \[\] \[Event \{\d+\}: TargetSet')
cur = None
last = None
for line in open(f, encoding='cp1252', errors='replace'):
    m = area_re.search(line)
    if m:
        cur = m.group(1)
        continue
    if cur != log_area:
        continue
    m = tgt.search(line)
    if m:
        last = (ts(m.group(1)), m.group(2), int(m.group(3)), tuple(float(m.group(i)) for i in (5, 6, 7)))
        continue
    if 'ApplyEffect' in line and 'In Conversation' in line:
        t = ts(line[1:13])
        if not last or t - last[0] > 0.5:
            print(line[1:13], 'no TargetSet just before')
            continue
        _, name, nid, p = last
        fq = by_string.get(nid >> 32, set())
        cands = [r for x in fq for r in by_entity.get(x, [])]
        own = min((math.dist(p, lp(r)), r['spawn']) for r in cands) if cands else (None, None)
        anyr = sorted(rows, key=lambda r: math.dist(p, lp(r)))[:2]
        print('%s dt=%3dms %-26s own=%s %s | nearest %.2f %s | next %.2f' % (
            line[1:13], (t - last[0]) * 1000, name[:26],
            '%.2f' % own[0] if own[0] is not None else '-', own[1],
            math.dist(p, lp(anyr[0])), anyr[0]['spawn'], math.dist(p, lp(anyr[1]))))
