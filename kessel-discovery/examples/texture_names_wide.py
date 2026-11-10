"""texture_names_wide -- every texture-like path the client spells anywhere we
can read, hashed and matched to DDS entries (no dictionary).

Sources: area/room .dat, .epp, .fxspec, .mat (raw bytes, ASCII and UTF-16)
and kessel objects.json strings (GOM). Candidate = any run containing art/,
gfx/ or world/ path segments; each becomes /resources/<path>.dds (extension
stripped first; 'img://' prefixes dropped) and is hashed.

  python3 -I texture_names_wide.py <census.tsv> <hash_paths> <spice.sqlite> <dir> [<dir> ...]
"""
import collections, json, os, re, sqlite3, subprocess, sys

cen, hp, spice, dirs = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4:]
dds = set(); dds_arch = {}
for l in open(cen, newline=''):
    a, h, comp, cs, us, c, head = l.rstrip('\n').split('\t')
    if c == 'DDS':
        dds.add(h); dds_arch[h] = a
pat = re.compile(r'(?:img://)?[\\/]?(?:resources[\\/])?((?:art|gfx|world)[\\/][A-Za-z0-9_\\/.\-]+)')
cands = collections.defaultdict(set)


def add(s, src):
    for m in pat.finditer(s):
        v = m.group(1).replace('\\', '/').lower().rstrip('/.')
        v = re.sub(r'\.[a-z0-9]{2,4}$', '', v)
        cands[src].add('/resources/' + v + '.dds')


for d in dirs:
    tag = os.path.basename(d.rstrip('/'))
    for fn in os.listdir(d):
        b = open(os.path.join(d, fn), 'rb').read()
        add(b.decode('latin1'), tag)
        if b'\0' in b[:400]:
            add(b.decode('utf-16-le', 'replace'), tag)
db = sqlite3.connect(f'file:{spice}?mode=ro', uri=True)
for (j,) in db.execute("select json from objects where json like '%art/%' or json like '%art\\%' or json like '%gfx%'"):
    for s in json.loads(j).get('strings') or []:
        if isinstance(s, str):
            add(s, 'gom')
allp = sorted(set().union(*cands.values()))
r = subprocess.run([hp], input='\n'.join(allp), capture_output=True, text=True).stdout
H = {p: h for h, p in (x.split('\t', 1) for x in r.splitlines())}
named = set(); R = {}
for k, ps in cands.items():
    hs = {H[p] for p in ps} & dds
    named |= hs
    R[k] = {'candidates': len(ps), 'dds_hits': len(hs)}
R['union_named'] = len(named); R['dds'] = len(dds)
rest = collections.Counter(dds_arch[h] for h in dds - named)
R['unnamed_by_archive_top'] = rest.most_common(12)
print(json.dumps(R, indent=1))
open(os.path.join(os.path.dirname(cen), 'dds_named_wide.txt'), 'w').write('\n'.join(sorted(named)))
