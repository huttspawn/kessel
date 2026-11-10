"""usage: python3 -I gfx_idscan.py <dump dir with gfx.tsv and gfx/> docs/fields/registry.jsonl [control tokens...]

Scan every UI movie's AVM1 pushes and constant pools for the ids, name hashes and sample
values of every decoded-likely and unsettled field."""
import sys, json, collections
sys.path.insert(0, __file__.rsplit('/', 1)[0])
import gfx_dump, gfx_as2_disasm as dis
U = sys.argv[1]; reg = sys.argv[2]
want = {}
for l in open(reg):
    r = json.loads(l)
    if r['status'] not in ('decoded-likely', 'unsettled'):
        continue
    i = int(r['id'], 16)
    for v in (i, i & 0xffffffff):
        want[float(v)] = r['id']
    if r.get('name_hash'):
        want[float(int(r['name_hash'], 16))] = r['id']
    for s in (r['id'], r['id'].upper(), r.get('name_hash') or 'zzzz'):
        want[s] = r['id']
for c in sys.argv[3:]:
    want[c] = 'CONTROL'; want[float(c) if c.replace('.','').isdigit() else c] = 'CONTROL'
nmov = 0; hits = collections.Counter(); blocks = 0
for line in open(U + '/gfx.tsv'):
    h, size, magic, path = line.rstrip('\n').split('\t')
    if not magic.startswith('CFX'):
        continue
    nmov += 1
    _, body = gfx_dump.read_body(open(f'{U}/gfx/{h}.bin', 'rb').read())
    for code, d in gfx_dump.tags(body):
        if code not in (12, 59):
            continue
        blocks += 1
        try:
            ops = dis.disasm(d, 2 if code == 59 else 0)
        except Exception:
            continue
        for _, op, arg in ops:
            if op not in ('Push', 'ConstantPool'):
                continue
            for v in [x.strip().strip("'") for x in arg.split(',')]:
                try:
                    key = float(v)
                except ValueError:
                    key = v
                if key in want and not (isinstance(key, float) and key < 65536):
                    hits[(path.rsplit('/', 1)[-1], want[key], v)] += 1
print('movies', nmov, 'code blocks', blocks, 'targets', len(set(want.values())))
for k, n in hits.most_common(50):
    print(k, n)
print('hits', len(hits))
