"""Which area dir holds the player while the log says 'Space {945872057663605}'?
Compare logged positions (world = log/10, order x,z,height) with placed-instance positions per candidate dir.
usage: a8_space.py <areas_dir> <logdir>... -- <dir ids>"""
import glob, math, os, re, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from world_room import instances  # noqa: E402

i = sys.argv.index('--')
areas, logdirs, cands = sys.argv[1], sys.argv[2:i], sys.argv[i + 1:]
POS = {'4f77e269', '69e2774f'}
pts = {}
for d in cands:
    ps = []
    for f in glob.glob(os.path.join(areas, d, '*.dat')):
        if f.endswith('area.dat'):
            continue
        b = open(f, 'rb').read()
        for inst in instances(b):
            for p in inst[4]:
                if len(p) == 3 and p[1] in POS and isinstance(p[2], tuple):
                    ps.append(p[2])
    pts[d] = ps
    print(d, 'instances with position', len(ps))
pl = re.compile(r"^\[[\d:.]+\] \[@([^#|\]]+)#\d+\|\(([-\d.]+),([-\d.]+),([-\d.]+),[-\d.]+\)")
area_re = re.compile(r'AreaEntered \{\d+\}: ([^{]*)\{(\d+)\}\)? ?(\(\w+\))?')
res = {}
for ld in logdirs:
    for f in glob.glob(os.path.join(ld, '*.txt')):
        cur, tag, n = None, None, 0
        for line in open(f, encoding='cp1252', errors='replace'):
            m = area_re.search(line)
            if m:
                cur = m.group(2)
                tag = re.search(r'\((he\d+)\)', line)
                tag = tag.group(1) if tag else None
                n = 0
                continue
            if cur != '945872057663605' or n > 30:
                continue
            m = pl.match(line)
            if not m:
                continue
            n += 1
            a, b_, c = (float(m.group(k)) for k in (2, 3, 4))
            w = (a / 10, c / 10, b_ / 10)  # x, height, z
            for d, ps in pts.items():
                if ps:
                    dist = min(math.dist(w, q) for q in ps)
                    res.setdefault((m.group(1), tag, d), []).append(dist)
for k, v in sorted(res.items()):
    v.sort()
    print(k, 'n', len(v), 'median nearest instance', round(v[len(v) // 2], 2))
