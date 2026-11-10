"""Strict caller scan for orphan On Custom roots: a payload is a candidate caller of
(root script S, handler 'On Custom:X') when it holds X as a whole token AND references S, by
its fqn text or by S's GUID (8 bytes, either byte order). Covers every object kind, canonical or
not (non-canonical ability variants included), so callers in undecoded fields show up.
usage: orphan_scan2.py <spice.sqlite> <orphan_roots.tsv> <out.tsv>"""
import sys, sqlite3, base64, re, collections, json, struct
db, roots_tsv, out = sys.argv[1:4]
con = sqlite3.connect(db)
guid = {f: int(g, 16) for f, g in con.execute("select fqn, guid from objects where kind='Hydra' and is_canonical=1")}
roots = []
for l in open(roots_tsv):
    s, h, codes, achs = l.rstrip('\n').split('\t')
    if h.startswith('On Custom:'):
        roots.append((s, h[len('On Custom:'):], codes, achs))
by_name = collections.defaultdict(list)
for r in roots:
    by_name[r[1]].append(r)
names = sorted(by_name, key=len, reverse=True)
rx = re.compile(b'|'.join(re.escape(n.encode()) for n in names))
need = {}
for s, *_ in roots:
    pats = [s.encode()]
    if s in guid:
        pats += [struct.pack('>Q', guid[s]), struct.pack('<Q', guid[s])]
    need[s] = pats
hits = collections.defaultdict(list)
for fqn, kind, canon, b64 in con.execute("select fqn, kind, is_canonical, json_extract(json,'$.payload_b64') from objects"):
    if not b64:
        continue
    data = None
    raw = base64.b64decode(b64)
    found = set()
    for m in rx.finditer(raw):
        b = raw[m.end():m.end() + 1]
        a = raw[m.start() - 1:m.start()]
        if (b and (b.isalnum() or b == b'_')) or (a and (a.isalnum() or a == b'_')):
            continue
        found.add(m.group(0).decode())
    for name in found:
        for s, _, codes, achs in by_name[name]:
            if fqn == s:
                continue
            how = [i for i, p in enumerate(need[s]) if p in raw]
            if how:
                hits[(s, name)].append((kind, fqn, canon, 'fqn' if 0 in how else 'guid'))
c = collections.Counter(); kinds = collections.Counter()
with open(out, 'w') as o:
    for s, name, codes, achs in roots:
        hs = hits.get((s, name), [])
        c['caller found' if hs else 'none'] += 1
        for h in hs:
            kinds[h[0]] += 1
        o.write('%s\tOn Custom:%s\t%s\t%s\t%s\n' % (s, name, codes, json.dumps(hs[:8]), achs))
print(dict(c), dict(kinds))
