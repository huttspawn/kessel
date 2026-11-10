"""Scan every object payload in an extract (all kinds, buckets and prototypes alike) for GUID references.

usage: python3 -I object_guid_scan.py <extract.sqlite> <guids.txt>

guids.txt: one GUID per line, hex, optionally CF-prefixed (era_condition_refs.py writes one). A GUID
reference in a GOM payload is 0xCF followed by the 8-byte big-endian GUID; the little-endian form is
searched too. Prints each hit (object kind, fqn, guid) and a count per kind of objects scanned.
"""
import sys, sqlite3, base64, json, collections

DB, GUIDS = sys.argv[1], sys.argv[2]
gs = [l.strip().upper().removeprefix('CF') for l in open(GUIDS) if l.strip()]
pats = {}
for g in gs:
    v = int(g, 16)
    pats[b'\xcf' + v.to_bytes(8, 'big')] = g
    pats[v.to_bytes(8, 'little')] = g
scanned = collections.Counter(); hits = []
for kind, fqn, js in sqlite3.connect(DB).execute('select kind, fqn, json from objects'):
    try:
        b64 = json.loads(js).get('payload_b64')
    except (ValueError, AttributeError):
        b64 = None
    if not b64:
        continue
    data = base64.b64decode(b64); scanned[kind] += 1
    for p, g in pats.items():
        if p in data:
            hits.append((kind, fqn, g))
print('objects scanned', sum(scanned.values()), dict(scanned.most_common(12)))
for h in hits:
    print('HIT', h)
print('hits', len(hits), 'distinct guids hit', len({h[2] for h in hits}), 'of', len(gs))
