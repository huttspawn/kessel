"""For each many-to-one log area: describe each candidate dir (rooms, phase naming it, placements, map pages).
usage: a8_many.py <a8_logs.txt> <areas_dir> <phs.jsonl> <spice>"""
import ast, collections, json, os, re, sqlite3, sys

areas = sys.argv[2]
db = sqlite3.connect('file:' + sys.argv[4] + '?mode=ro', uri=True)
phs = collections.defaultdict(list)
for l in open(sys.argv[3]):
    o = json.loads(l)
    for k in ('24ed04cd', '24ed04cc'):
        if isinstance(o['v'].get(k), int):
            phs[o['v'][k]].append((k, o['fqn']))
for line in open(sys.argv[1]):
    if not line.startswith('MANY') and not line.startswith('MISS'):
        continue
    m = re.match(r'(\w+) (\d+) \d+ \d+ (.*?) \d+ (\[.*\])$', line.strip())
    tag, lid, name, rows = m.group(1), m.group(2), m.group(3), ast.literal_eval(m.group(4))
    print(tag, lid, name)
    for _, d in rows:
        p = os.path.join(areas, str(d))
        fs = os.listdir(p) if os.path.isdir(p) else []
        rooms = [f[:-4] for f in fs if f.endswith('.dat') and f not in ('area.dat', '_everywhere_.dat', 'default.dat')]
        pl = db.execute('select count(*) from placements where area_dir_id=?', (d,)).fetchone()[0]
        nm = 0
        if 'mapnotes.not' in fs:
            nm = open(os.path.join(p, 'mapnotes.not'), encoding='utf-8', errors='replace').read().count('mpnPosition')
        print('   ', d, 'rooms', len(rooms), rooms[:4], 'placements', pl, 'notes', nm, 'phases', phs.get(d, [])[:3])
