"""Nearest spawner instances to a log position. usage: near.py <area jsonl> a b c [n]"""
import json, math, sys
rows = [json.loads(l) for l in open(sys.argv[1])]
p = tuple(float(x) for x in sys.argv[2:5])
n = int(sys.argv[5]) if len(sys.argv) > 5 else 5
def logpos(r):
    w = r['world']
    return (w[0] * 10, w[2] * 10, w[1] * 10)
rows = [r for r in rows if r['world']]
rows.sort(key=lambda r: math.dist(p, logpos(r)))
for r in rows[:n]:
    print('%.2f' % math.dist(p, logpos(r)), r['spawn'], r['spawner_type'], r['entities'], r['room'], r['instance'], tuple(round(x, 2) for x in logpos(r)))
