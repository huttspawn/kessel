"""In the two combat-style conversations, list custom-event strings in payload order with the nearest preceding condition/object GUID refs."""
import json, sys, sqlite3, base64, struct, re
db = sys.argv[1]
con = sqlite3.connect(db)
guids = {}
for g, f in con.execute('select guid, fqn from objects'):
    try:
        guids[int(g, 16)] = f
    except Exception:
        pass
for cnv in ('cnv.character.combat_style.force_reflection', 'cnv.character.combat_style.second_tech_style'):
    js = con.execute('select json from objects where fqn=?', (cnv,)).fetchone()[0]
    p = base64.b64decode(json.loads(js)['payload_b64'])
    print('==', cnv, len(p))
    events = []
    for m in re.finditer(rb'(Grant|Swap to) [A-Z][a-z]+', p):
        events.append((m.start(), 'EVENT ' + m.group().decode()))
    for i in range(len(p) - 8):
        v = struct.unpack('>Q', p[i:i + 8])[0]
        if v in guids and v >> 56 == 0xE0:
            events.append((i, 'REF ' + guids[v]))
    for off, e in sorted(events):
        print(off, e)
