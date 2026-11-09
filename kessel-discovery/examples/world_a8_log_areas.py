"""A8 log side: every AreaEntered id in the logs, how many area dirs it joins to, and which source.
usage: a8_logs.py <spice> <logdir>..."""
import collections, glob, os, re, sqlite3, sys

db = sqlite3.connect('file:' + sys.argv[1] + '?mode=ro', uri=True)
area_re = re.compile(r'AreaEntered \{\d+\}: ([^{]*)\{(\d+)\}')
seen = collections.Counter()
names = {}
for d in sys.argv[2:]:
    for f in glob.glob(os.path.join(d, '*.txt')):
        for line in open(f, encoding='cp1252', errors='replace'):
            m = area_re.search(line)
            if m:
                lid = int(m.group(2))
                seen[lid] += 1
                names[lid] = m.group(1).strip()
cols = [r[1] for r in db.execute("pragma table_info(area_log_ids)")]
print('area_log_ids columns', cols)
many = 0
for lid in sorted(seen, key=lambda x: names[x]):
    rows = db.execute('select * from area_log_ids where log_area_id=?', (lid,)).fetchall()
    tag = 'MISS' if not rows else ('MANY' if len({r[cols.index('area_dir_id')] for r in rows}) > 1 else 'ONE')
    many += tag == 'MANY'
    print(tag, lid, lid >> 32, lid & 0xffffffff, names[lid], seen[lid], [tuple(r) for r in rows][:6])
print('log areas', len(seen), 'many', many, 'miss', sum(1 for l in seen if not db.execute('select 1 from area_log_ids where log_area_id=?', (l,)).fetchone()))
