"""wwise_bank_names -- name every bank entry without the dictionary
(#429 criterion 7, audio).

A bank's id (BKHD +0x0C) is FNV-1 32 of its lowercased name, and its archive
path is /resources/bnk2/<name>.bnk. Names come from STID sections of other
banks; for the banks no STID names, candidates are drawn from client text:
every identifier token in GOM object strings (kessel objects), in the given
dirs (epp/dat/fxspec raw bytes), STID names with _data/_media swapped, and
'voc_' + every soundset token. A candidate names a bank when its FNV-1 equals
the bank id.

  python3 -I wwise_bank_names.py <wwise.json> <spice.sqlite> <dir> [<dir> ...]
"""
import json, os, re, sqlite3, sys

wj, spice, dirs = sys.argv[1], sys.argv[2], sys.argv[3:]
W = json.load(open(wj))
named = {int(k) for k in W['bank_names']}
want = {b['bank_id']: h for h, b in W['per_bank'].items() if b['bank_id'] not in named}


def fnv(s):
    h = 2166136261
    for c in s.lower().encode():
        h = (h * 16777619) & 0xffffffff
        h ^= c
    return h


tok = re.compile(rb'[A-Za-z][A-Za-z0-9_]{2,80}')
cands = set()
for n in W['bank_names'].values():
    cands.add(n.replace('_data', '_media')); cands.add(n.replace('_media', '_data'))
db = sqlite3.connect(f'file:{spice}?mode=ro', uri=True)
for (j,) in db.execute('select json from objects'):
    for s in json.loads(j).get('strings') or []:
        if isinstance(s, str):
            for t in tok.findall(s.encode('utf-8', 'replace')):
                cands.add(t.decode())
for d in dirs:
    for fn in os.listdir(d):
        b = open(os.path.join(d, fn), 'rb').read()
        for t in tok.findall(b):
            cands.add(t.decode())
        if b'\0' in b[:400]:
            for t in tok.findall(b.decode('utf-16-le', 'replace').encode('utf-8', 'replace')):
                cands.add(t.decode())
more = set()
for c in cands:
    for pre in ('voc_', 'location_', 'vox_', 'voc_pet_'):
        more.add(pre + c.lower())
    for suf in ('_media', '_data'):
        more.add(c.lower() + suf)
cands |= more
hit = {}
for c in cands:
    i = fnv(c)
    if i in want:
        hit.setdefault(i, c.lower())
print(json.dumps({'unnamed_by_stid': len(want), 'named_from_client_text': len(hit),
                  'still_unnamed': len(want) - len(hit), 'names': sorted(hit.values())}, indent=1))
