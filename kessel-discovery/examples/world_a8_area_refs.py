"""A8: which area directory ids are named anywhere in GOM data, and by what field.
Scans every decoded jsonl given plus raw payloads of every object in the db for the 18 missing ids.
usage: a8_refs.py <spice> <areas_dir> <missing18.txt> <jsonl>..."""
import base64, collections, json, os, struct, sqlite3, sys

spice, areas, missing_f = sys.argv[1:4]
dirs = {int(d) for d in os.listdir(areas)}
missing = {int(x) for x in open(missing_f).read().split()}
hits = collections.defaultdict(collections.Counter)


def walk(v, path, fqn):
    if isinstance(v, dict):
        for k, x in v.items():
            walk(x, k, fqn)
    elif isinstance(v, list):
        for x in v:
            walk(x, path, fqn)
    elif isinstance(v, int) and v in dirs:
        hits[v][(path, fqn.split('.')[0])] += 1


for f in sys.argv[4:]:
    for l in open(f):
        o = json.loads(l)
        walk(o['v'], '', o['fqn'])
named = set(hits)
print('area dirs', len(dirs), 'named by a decoded field', len(named))
print('missing18 named by decoded fields:', {m: dict(hits[m]) for m in missing if m in hits})
fields = collections.Counter()
for d, c in hits.items():
    for (p, k), n in c.items():
        fields[(p, k)] += 1
print('fields naming area dirs (dirs named):', fields.most_common(25))

# raw scan of every object payload for the 18 ids as u64 LE and BE and the CF-prefixed form
db = sqlite3.connect('file:' + spice + '?mode=ro', uri=True)
pats = {}
for m in missing:
    pats[struct.pack('<Q', m)] = (m, 'le')
    pats[struct.pack('>Q', m)] = (m, 'be')
raw = collections.defaultdict(list)
for fqn, kind, js in db.execute("select fqn, kind, json from objects"):
    try:
        b = base64.b64decode(json.loads(js).get('payload_b64') or '')
    except Exception:
        continue
    for p, (m, e) in pats.items():
        if p in b:
            raw[m].append((fqn, kind, e))
print('missing18 found raw in object payloads:')
for m in sorted(missing):
    print(' ', m, raw.get(m, [])[:6])
