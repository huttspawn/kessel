"""The 135 'Placeholder' follower influence achievements: their task count for the event, and who
references them (any payload holding the achievement's GUID, either byte order, or its fqn), in
live and PTS. usage: follower_ach.py <spice.sqlite> <Achievement.jsonl>"""
import sys, sqlite3, base64, json, struct, collections
db, achj = sys.argv[1:3]
con = sqlite3.connect(db)
PH = -2305566688057113569
counts = collections.Counter(); fq = []
for line in open(achj):
    o = json.loads(line)
    for t in o['v'].get('8ce6ac36') or []:
        for e in (t.get('v') or {}).get('8ce6ac37') or []:
            if e.get('k') == PH:
                counts[e.get('v')] += 1; fq.append(o['fqn'])
print('Placeholder tasks by required count', dict(counts), 'achievements', len(set(fq)))
g = {f: int(x, 16) for f, x in con.execute("select fqn, guid from objects where kind='Achievement' and is_canonical=1")}
pats = {}
for f in set(fq):
    if f in g:
        pats[f] = [struct.pack('>Q', g[f]), struct.pack('<Q', g[f]), f.encode()]
hold = collections.defaultdict(list)
for fqn, kind, b64 in con.execute("select fqn, kind, json_extract(json,'$.payload_b64') from objects"):
    if not b64:
        continue
    raw = base64.b64decode(b64)
    for f, ps in pats.items():
        if fqn != f and any(p in raw for p in ps):
            hold[f].append((kind, fqn))
for fqn, b64 in con.execute('select fqn, payload_b64 from singletons'):
    if b64:
        raw = base64.b64decode(b64)
        for f, ps in pats.items():
            if any(p in raw for p in ps):
                hold[f].append(('singleton', fqn))
kinds = collections.Counter(k for v in hold.values() for k, _ in v)
print('achievements with a referrer', len(hold), 'of', len(pats), 'referrer kinds', dict(kinds))
for f, v in list(hold.items())[:6]:
    print('  ', f, v[:4])
