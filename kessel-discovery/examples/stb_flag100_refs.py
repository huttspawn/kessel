"""Is an STB string (id2 = object string_id) referenced by an object, split by 0x100 and text.
And for referenced ones, what share of the referencing objects are test/dev fqns.
usage: ref100.py <stb_root> <spice.sqlite> table:kind ..."""
import os, sys, sqlite3, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from stb_entries import entries

root, db = sys.argv[1:3]
con = sqlite3.connect(db)
DEV = ('test', 'qa', 'deprecated', 'old', 'cut', 'dev', 'proto', 'placeholder', 'unused', 'temp')
for spec in sys.argv[3:]:
    t, kind = spec.split(':')
    sid = collections.defaultdict(list)
    for fqn, s in con.execute('select fqn, string_id from objects where kind=? and is_canonical=1 and string_id is not null', (kind,)):
        sid[int(s)].append(fqn)
    c = collections.Counter()
    for e in entries(os.path.join(root, 'resources/en-us/str', t + '.stb')):
        if e[2] & 0xff != 0x41:
            continue
        k = ('0x100' if e[2] & 0x100 else 'none', 'text' if e[6] else 'empty')
        refs = sid.get(e[1], [])
        if not refs:
            c[k + ('unreferenced',)] += 1
        elif all(any(d in seg for seg in f.split('.') for d in DEV) for f in refs):
            c[k + ('dev-only refs',)] += 1
        else:
            c[k + ('live refs',)] += 1
    print('==', t, kind)
    for k in sorted(c):
        print('   ', k, c[k])
