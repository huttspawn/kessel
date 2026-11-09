"""C5: the .node paths in the dictionary that are absent from the archives -- what GUIDs are they?
usage: c5_nodes.py <hashes_filename.txt> <nodes_dir> <pinf.tsv> <unfiltered>"""
import collections, os, sqlite3, sys

paths = [l.split('#')[2] for l in open(sys.argv[1], encoding='utf-8', errors='replace') if '.node#' in l]
present = {f[:-5] for f in os.listdir(sys.argv[2])}
pinf = dict(l.split() for l in open(sys.argv[3]))
un = sqlite3.connect('file:' + sys.argv[4] + '?mode=ro', uri=True)
obj = {}
for fqn, kind, guid in un.execute("select fqn, kind, guid from objects"):
    obj.setdefault(guid, (fqn, kind))
st = collections.Counter()
ex = collections.defaultdict(list)
for p in paths:
    n = os.path.basename(p)[:-5]
    g = '%016X' % int(n)
    here = 'present' if n in present else 'absent'
    o = obj.get(g)
    key = (here, 'pinf ' + pinf.get(g, '-'), (o[1] if o else 'no object'))
    st[key] += 1
    if len(ex[key]) < 3:
        ex[key].append((n, o[0] if o else None))
for k, v in st.most_common():
    print(v, k, ex[k])
