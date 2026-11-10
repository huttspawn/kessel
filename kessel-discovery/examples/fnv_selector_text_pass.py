"""Widest text pass for open FNV-1a 64 ids: every printable run (3..80 bytes) in every object and
singleton payload and client.gom, every object fqn and its '.'-segments and suffixes, each
uppercased as is, with '_'<->' ' swapped, and with '_N'/' N'/N suffixes 0..9.
usage: wide_text.py <spice.sqlite> <client.gom.bin> <id,id,...>"""
import sys, sqlite3, base64, re
db, gom, ids = sys.argv[1:4]
T = {int(x) for x in ids.split(',')}
P = 0x100000001b3; M = (1 << 64) - 1


def f(b):
    h = 0xcbf29ce484222325
    for c in b:
        h ^= c; h = (h * P) & M
    return h - (1 << 64) if h >> 63 else h


seen = set(); n = 0; hits = {}


def test(s):
    global n
    if s in seen:
        return
    seen.add(s)
    n += 1
    for v in (s, s.replace(b'_', b' '), s.replace(b' ', b'_')):
        h = f(v)
        if h in T:
            hits[h] = v; print('HIT', h, v, flush=True)


rx = re.compile(rb'[\x20-\x7e]{3,80}')
con = sqlite3.connect(db)


def blob(b):
    for m in rx.finditer(b):
        s = m.group(0).upper()
        test(s)
        for part in re.split(rb'[.:/\\]', s):
            if len(part) >= 3:
                test(part)


for (fqn,) in con.execute('select fqn from objects'):
    u = fqn.upper().encode()
    segs = u.split(b'.')
    for i in range(len(segs)):
        test(b'.'.join(segs[i:]))
        test(segs[i])
for (b64,) in con.execute("select json_extract(json,'$.payload_b64') from objects"):
    if b64:
        blob(base64.b64decode(b64))
for (b64,) in con.execute('select payload_b64 from singletons'):
    if b64:
        blob(base64.b64decode(b64))
blob(open(gom, 'rb').read())
print('distinct strings tested', n, 'hits', hits)
