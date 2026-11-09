"""Search world files for the missing area ids, binary (LE/BE) and decimal text (ascii and utf-16).
usage: a8_files.py <missing18> <file-or-dir>..."""
import os, struct, sys, collections

ids = [int(x) for x in open(sys.argv[1]).read().split()]
pats = {}
for m in ids:
    pats[struct.pack('<Q', m)] = (m, 'le')
    pats[struct.pack('>Q', m)] = (m, 'be')
    pats[str(m).encode()] = (m, 'dec')
    pats[str(m).encode('utf-16-le')] = (m, 'dec16')
hits = collections.defaultdict(list)


def scan(p):
    b = open(p, 'rb').read()
    own = os.path.basename(os.path.dirname(p))
    for pat, (m, e) in pats.items():
        if pat in b and own != str(m):
            hits[m].append((p.split('/areas/')[-1] if '/areas/' in p else p, e))


for root in sys.argv[2:]:
    if os.path.isfile(root):
        scan(root)
        continue
    for dp, _, fs in os.walk(root):
        for f in fs:
            if f.endswith('.dds'):
                continue
            scan(os.path.join(dp, f))
for m in ids:
    print(m, len(hits[m]), hits[m][:6])
