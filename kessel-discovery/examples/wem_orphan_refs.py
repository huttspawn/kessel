"""wem_orphan_refs -- after wem_id_sweep names every streamed .wem, check the
sweep against the bank-derived ids and look for the orphans' ids anywhere else
in the client (#429 criterion 7, audio).

  1. agreement: every RIFF entry the HIRC u32 scan reached (wwise_orphans)
     must carry the same id in the sweep;
  2. orphans (sweep ids no HIRC u32 holds): are they in any bank's DIDX, in
     any bank's non-HIRC bytes (u32 LE), in any .epp/.dat/.fxspec file (u32 LE
     or decimal text), or in any GOM object's strings / payload (kessel
     objects table)?

  python3 -I wem_orphan_refs.py <wem_ids.tsv> <bank_dir> <wwise.json> <spice.sqlite> <dir> [<dir> ...]
"""
import base64, collections, json, os, re, sqlite3, struct, sys

ids_f, bdir, wj, spice, dirs = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5:]
sweep = {}
for l in open(ids_f):
    h, n = l.split()
    sweep[h] = int(n)
W = json.load(open(wj))
didx = set()
for b in W['per_bank'].values():
    didx.update(b.get('didx_ids') or [])

import numpy as np

all_ids = np.array(sorted(set(sweep.values())), dtype=np.uint32)


def u32_hits(body, ids):
    """Every id in `ids` occurring as a u32 LE at any byte offset of body."""
    out = set()
    for i in range(4):
        n = (len(body) - i) // 4
        if n <= 0:
            continue
        a = np.frombuffer(body, dtype='<u4', count=n, offset=i)
        out.update(int(x) for x in np.unique(a[np.isin(a, ids)]))
    return out


hirc_u32 = set()
other_u32 = set()  # u32 LE in a bank chunk other than HIRC and DATA
for fn in os.listdir(bdir):
    b = open(os.path.join(bdir, fn), 'rb').read()
    if b[:4] != b'BKHD':
        continue
    q = 0
    while q + 8 <= len(b):
        tag, size = b[q:q + 4], struct.unpack_from('<I', b, q + 4)[0]
        if tag != b'DATA':
            (hirc_u32 if tag == b'HIRC' else other_u32).update(u32_hits(b[q + 8:q + 8 + size], all_ids))
        q += 8 + size
    del b

reached = {h: n for h, n in sweep.items() if n in hirc_u32}
orph = {h: n for h, n in sweep.items() if n not in hirc_u32}
R = {'swept': len(sweep), 'reached_by_hirc_u32': len(reached), 'orphans': len(orph)}
R['orphans_in_some_didx'] = sum(1 for n in orph.values() if n in didx)
R['orphans_in_bank_non_hirc_bytes'] = sum(1 for n in orph.values() if n in other_u32)

on = sorted(set(orph.values()))
ons = set(on)
on_np = np.array(on, dtype=np.uint32)
num = re.compile(rb'(?<![0-9])[0-9]{1,10}(?![0-9])')
hits = collections.defaultdict(lambda: collections.Counter())
for d in dirs:
    tag = os.path.basename(d.rstrip('/'))
    for fn in os.listdir(d):
        b = open(os.path.join(d, fn), 'rb').read()
        for n in u32_hits(b, on_np):
            hits[tag + ':u32'][n] += 1
        texts = [b]
        if b[:2] == b'\xff\xfe' or b'\0' in b[:400]:
            texts.append(b.decode('utf-16-le', 'replace').encode('ascii', 'replace'))
        for t in texts:
            for m in num.findall(t):
                if int(m) in ons:
                    hits[tag + ':text'][int(m)] += 1
db = sqlite3.connect(f'file:{spice}?mode=ro', uri=True)
for (j,) in db.execute('select json from objects'):
    o = json.loads(j)
    for s in o.get('strings') or []:
        if isinstance(s, str) and s.isdigit() and int(s) in ons:
            hits['gom:string'][int(s)] += 1
    p = o.get('payload_b64')
    if p:
        for n in u32_hits(base64.b64decode(p), on_np):
            hits['gom:payload_u32'][n] += 1
R['orphan_hits'] = {k: {'distinct_ids': len(c), 'uses': sum(c.values())} for k, c in hits.items()}
R['orphans_with_any_hit'] = len(set().union(*[set(c) for c in hits.values()])) if hits else 0
R['orphan_id_range'] = [min(on), max(on)] if on else None
R['reached_id_range'] = [min(reached.values()), max(reached.values())] if reached else None
print(json.dumps(R, indent=1))
json.dump({'orphans': orph, 'hits': {k: dict(c) for k, c in hits.items()}}, open(ids_f + '.orphans.json', 'w'))
