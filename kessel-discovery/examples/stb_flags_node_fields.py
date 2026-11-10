"""Over every cnv node with a line: for each STB flag bit, which node fields best predict it
(precision and recall). usage: bit_fields.py <lines.tsv> <stb_root>"""
import os, sys, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from stb_entries import entries

lines_tsv, stb_root = sys.argv[1:3]
by = collections.defaultdict(list)
for l in open(lines_tsv):
    p = l.rstrip('\n').split('\t')
    if p[1] == '0' and p[3]:
        by[p[0]].append((int(p[2]), int(p[3]), set(p[8].split(',')), p[5] != ''))
bitn = collections.Counter(); fld = collections.Counter(); both = collections.Counter(); total = 0
samples = collections.defaultdict(list)
for fqn, rows in by.items():
    stb = os.path.join(stb_root, 'resources/en-us/str/' + fqn.replace('.', '/') + '.stb')
    if not os.path.exists(stb):
        continue
    fl = {e[0]: (e[2], e[6]) for e in entries(stb) if e[2] & 0xff == 0x41}
    for num, line, fields, alien in rows:
        if line not in fl:
            continue
        f, text = fl[line]
        total += 1
        fields = set(fields)
        if text:
            fields.add('TEXT')
        for x in fields:
            fld[x] += 1
        for bit in (0x100, 0x400, 0x800, 0x1000):
            if f & bit:
                bitn[bit] += 1
                for x in fields:
                    both[(bit, x)] += 1
                if bit == 0x1000 and len(samples[fqn]) < 3:
                    samples[fqn].append((num, text[:70]))
print('nodes with line', total)
for bit in (0x100, 0x400, 0x800, 0x1000):
    print('bit %#x set on %d' % (bit, bitn[bit]))
    sc = []
    for x, n in fld.items():
        b = both[(bit, x)]
        if b == 0:
            continue
        prec, rec = b / n, b / bitn[bit]
        sc.append((2 * prec * rec / (prec + rec), x, n, b, round(prec, 3), round(rec, 3)))
    for s in sorted(sc, reverse=True)[:6]:
        print('   field %s nodes %d both %d precision %.3f recall %.3f' % (s[1], s[2], s[3], s[4], s[5]))
top = sorted(samples.items(), key=lambda kv: -len(kv[1]))
print('0x1000 conversations', len(samples))
for k, v in list(samples.items())[:40]:
    print(k, v[:2])
