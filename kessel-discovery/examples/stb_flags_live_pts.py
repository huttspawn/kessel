"""Compare cnv STB primary (0x41) entries live vs PTS: flag changes, text changes, and how bit
0x1000 relates to them. usage: stb_live_pts.py <live_root> <pts_root>"""
import os, sys, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from stb_entries import entries

live, pts = sys.argv[1:3]
sub = 'resources/en-us/str/cnv'
c = collections.Counter(); ex = collections.defaultdict(list)
for dp, dn, fn in os.walk(os.path.join(pts, sub)):
    for f in fn:
        pp = os.path.join(dp, f)
        lp = os.path.join(live, os.path.relpath(pp, pts))
        if not os.path.exists(lp):
            c['new table'] += 1; continue
        L = {(e[0], e[1]): e for e in entries(lp) if e[2] & 0xff == 0x41}
        for e in entries(pp):
            if e[2] & 0xff != 0x41:
                continue
            l = L.get((e[0], e[1]))
            if l is None:
                c['new key'] += 1
                if e[2] & 0x1000:
                    c['new key with 0x1000'] += 1
                continue
            fl, fp = l[2], e[2]
            tchg = l[6] != e[6]
            k = ('live1000=%d' % bool(fl & 0x1000), 'pts1000=%d' % bool(fp & 0x1000), 'text changed' if tchg else 'same text')
            c[k] += 1
            if fl != fp:
                c[('flag change', hex(fl), hex(fp), 'text changed' if tchg else 'same text')] += 1
                if len(ex[(hex(fl), hex(fp))]) < 4:
                    ex[(hex(fl), hex(fp))].append((os.path.relpath(pp, pts), e[0], l[6][:50], '->', e[6][:50]))
            if tchg and len(ex[('text', bool(fl & 0x1000), bool(fp & 0x1000))]) < 4:
                ex[('text', bool(fl & 0x1000), bool(fp & 0x1000))].append((os.path.relpath(pp, pts), e[0], l[6][:50], '->', e[6][:50]))
for k, n in sorted(c.items(), key=lambda kv: str(kv[0])):
    print(n, k)
for k, v in ex.items():
    print(k)
    for x in v:
        print('   ', x)
