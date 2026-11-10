"""For the 4 text-named event codes and the karagga_survival raid state: where the name is held
(object payloads, singletons), which achievements count the code, and whether a quest/object of
that fqn exists in live or PTS. usage: text_named.py <live spice> <pts spice> <lgcach.json>"""
import sys, sqlite3, base64, json
live, pts, lgc = sys.argv[1:4]
M = (1 << 64) - 1
names = {'E00050242719610F': 'qst.location.operations.imperial.weekly_classic_operations',
         'E000E153A13EA3DA': 'qst.flashpoint.corellia.corellia_flashpoint_logic_republic',
         'E000ED86A2582BF1': 'qst.location.operations.republic.weekly_classic_operations',
         'E000FB500861C41F': 'Placeholder',
         '0008134B00000119': '8134b00000119'}
lg = {e['k'] & M: e['v'] for e in json.load(open(lgc))['55f64de0'] if isinstance(e['k'], int)}
for db, tag in ((live, 'live'), (pts, 'pts')):
    con = sqlite3.connect(db)
    print('=====', tag)
    for code, n in names.items():
        c = int(code, 16)
        objs = con.execute('select kind, fqn from objects where fqn=?', (n,)).fetchall()
        like = con.execute("select count(*) from objects where fqn like ?", (n.rsplit('.', 1)[0] + '.%',)).fetchone()[0] if '.' in n else 0
        print(code, n, 'consumers', lg.get(c), 'object', objs, 'siblings under parent', like)
    pats = {n: n.encode() for n in names.values()}
    pats['8134b00000119'] = str(0x8134b00000119).encode()
    hold = {n: [] for n in pats}
    for fqn, kind, b64 in con.execute("select fqn, kind, json_extract(json,'$.payload_b64') from objects"):
        if not b64:
            continue
        raw = base64.b64decode(b64)
        for n, p in pats.items():
            if p in raw:
                hold[n].append((kind, fqn))
    for fqn, b64 in con.execute('select fqn, payload_b64 from singletons'):
        if b64:
            raw = base64.b64decode(b64)
            for n, p in pats.items():
                if p in raw:
                    hold[n].append(('singleton', fqn))
    for n, h in hold.items():
        print('  held', n, len(h), h[:6])
