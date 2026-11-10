"""Census over every raw en-us STB: flag high byte x version x additional-length != text-length,
by table root; plus samples of 0x100-unset-with-text and 0x100-set-empty outside cnv.
usage: stb_census.py <stb_root>"""
import os, struct, sys, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from stb_entries import entries

root = os.path.join(sys.argv[1], 'resources/en-us/str')
c = collections.Counter(); ver = collections.Counter(); alen = collections.Counter()
s_unset_text = collections.defaultdict(list); s_set_empty = collections.defaultdict(list)
per_table = collections.defaultdict(collections.Counter)
for dp, dn, fn in os.walk(root):
    for f in fn:
        p = os.path.join(dp, f)
        rel = p[len(root) + 1:]
        top = rel.split('/')[0]
        for id1, id2, fl, v, tl, al, t in entries(p):
            hi = fl & 0xff00
            c[(top == 'cnv', fl & 0xff, hi)] += 1
            ver[(bool(fl & 0x1000), v)] += 1
            alen[(bool(fl & 0x1000), al == tl)] += 1
            if top != 'cnv' and fl & 0xff == 0x41:
                k = (bool(fl & 0x100), bool(t))
                per_table[rel][k] += 1
                if k == (False, True) and len(s_unset_text[rel]) < 2:
                    s_unset_text[rel].append((id1, id2, t[:50]))
                if k == (True, False) and len(s_set_empty[rel]) < 2:
                    s_set_empty[rel].append((id1, id2))
for k in sorted(c):
    print('cnv=%d kind=%02x hi=%04x %d' % (k[0], k[1], k[2], c[k]))
print('version by 0x1000:', ver.most_common(8))
print('addlen==textlen by 0x1000:', alen)
print('tables: (0x100 set?, has text) worst offenders')
rows = sorted(per_table.items(), key=lambda kv: -(kv[1][(False, True)] + kv[1][(True, False)]))
for rel, k in rows[:25]:
    print(' ', rel, dict(k), s_unset_text.get(rel, [])[:1], s_set_empty.get(rel, [])[:1])
