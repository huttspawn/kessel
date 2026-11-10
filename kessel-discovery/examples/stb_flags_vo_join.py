"""Join VO banks -> node number -> STB line -> flags.

usage: vo_join.py <lines.tsv> <vo_root> <stb_root> <rows_out.tsv>
"""
import os, re, sys, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from stb_entries import entries

lines_tsv, vo_root, stb_root, out = sys.argv[1:5]
bank = os.path.join(vo_root, 'resources/en-us/bnk2')
want = {f[:-4] for f in os.listdir(bank) if f.endswith('.acb')}
nodes = collections.defaultdict(list)  # acbname -> rows
for l in open(lines_tsv):
    fqn, link, num, line, pc, anode, acnv, spk = l.rstrip('\n').split('\t')
    name = 'cnv_' + fqn[4:].replace('.', '_')
    if name in want and link == '0' and num and line:
        nodes[name].append((int(num), int(line), pc == '1', bool(anode), spk, fqn))
tab = collections.Counter()
o = open(out, 'w')
banks = 0
for name, rows in sorted(nodes.items()):
    fqn = rows[0][5]
    stb = os.path.join(stb_root, 'resources/en-us/str/' + fqn.replace('.', '/') + '.stb')
    if not os.path.exists(stb):
        continue
    data = open(os.path.join(bank, name + '.acb'), 'rb').read()
    voiced = set()
    for m in re.finditer(re.escape(name.encode()) + rb'_(\d+)((?:_[a-z0-9]+)*)\.wem', data):
        voiced.add(int(m.group(1)))
    fl = {}
    for e in entries(stb):
        if e[2] & 0xff == 0x41:
            fl[e[0]] = (e[2], e[6])
    banks += 1
    for num, line, pc, alien, spk, _ in rows:
        if line not in fl:
            tab['line not in stb'] += 1
            continue
        f, text = fl[line]
        v = num in voiced
        o.write('%s\t%d\t%d\t%04x\t%d\t%d\t%d\t%d\t%s\n' % (fqn, num, line, f, v, pc, alien, len(text), text[:60]))
        for bit in (0x100, 0x400, 0x800, 0x1000):
            tab[(bit, bool(f & bit), v, bool(text), alien, pc)] += 1
print('banks', banks, 'line not in stb', tab['line not in stb'])
for bit in (0x100, 0x400, 0x800, 0x1000):
    print('bit %#x' % bit)
    for s in (True, False):
        def c(**k):
            return sum(n for key, n in tab.items() if isinstance(key, tuple) and key[0] == bit and key[1] == s
                       and all(key[{'v': 2, 't': 3, 'a': 4, 'p': 5}[x]] == y for x, y in k.items()))
        print('  set=%d all %7d | voiced %7d unvoiced %7d | text %7d notext %7d | voiced+text %7d voiced-notext %6d unvoiced+text %6d unvoiced-notext %7d | alien %6d pc %7d' % (
            s, c(), c(v=True), c(v=False), c(t=True), c(t=False), c(v=True, t=True), c(v=True, t=False), c(v=False, t=True), c(v=False, t=False), c(a=True), c(p=True)))
