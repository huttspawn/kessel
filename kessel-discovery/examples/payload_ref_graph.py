"""Find every object whose payload carries the GUID (either byte order) of a target fqn pattern; also list what each target payload references."""
import json, sys, sqlite3, base64, struct
db, pattern = sys.argv[1:3]
con = sqlite3.connect(db)
guids = {}
for g, f in con.execute('select guid, fqn from objects'):
    try:
        guids[int(g, 16)] = f
    except Exception:
        pass
targets = {int(g, 16): f for g, f in con.execute('select guid, fqn from objects where fqn like ?', (pattern,))}
needles = {}
for g, f in targets.items():
    needles[struct.pack('>Q', g)] = f
    needles[struct.pack('<Q', g)] = f
found = {}
out_refs = {}
for fqn, js in con.execute('select fqn, json from objects'):
    try:
        p = base64.b64decode(json.loads(js).get('payload_b64') or '')
    except Exception:
        continue
    for nd, f in needles.items():
        if fqn != f and nd in p:
            found.setdefault(f, set()).add(fqn)
    if fqn in targets.values():
        refs = set()
        for i in range(len(p) - 7):
            v = struct.unpack('>Q', p[i:i + 8])[0]
            if v in guids and v >> 56 == 0xE0 and guids[v] != fqn:
                refs.add(guids[v])
        out_refs[fqn] = refs
for f in sorted(targets.values()):
    print('==', f)
    print('   referenced by:', sorted(found.get(f, [])))
    print('   references:', sorted(out_refs.get(f, [])))
