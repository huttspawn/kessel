"""clo_census -- decode push 3 (#429): frame every cloth file (.clo, magic
'OLCB') and test its header on all of them.

Header (little endian), tested on every file:
  'OLCB' u32 version  u32 16 (base of every offset)  u32 body length (file length - 16)
  0x38 u32 node count  0x3C u32 a  0x40 u32 b
  0x58 u64 name table offset  0x60 u64 node record offset  0x68 u64 parameter offset
  (offsets relative to 0x10)
  name table: node count x 32-byte null-padded names; simulated nodes are
  'newtonparticle###', the others are skeleton bones the chain hangs from
The census prints what the counts relate to, so the record sizes can be read.

Usage: python3 -I clo_census.py <dir of OLCB files>
"""
import collections
import os
import struct
import sys


def main():
    src = sys.argv[1]
    c = collections.Counter()
    sizes = collections.Counter()
    anchors = collections.Counter()
    for fn in os.listdir(src):
        d = open(os.path.join(src, fn), 'rb').read()
        c['files'] += 1
        ver, base, body = struct.unpack_from('<III', d, 4)
        c[f'version {ver}'] += 1
        c['base 16' if base == 16 else 'x base not 16'] += 1
        c['body length == file - 16' if body == len(d) - 16 else 'x body length'] += 1
        n, a, b = struct.unpack_from('<III', d, 0x38)
        on, orec, opar = (x + 16 for x in struct.unpack_from('<QQQ', d, 0x58))
        if not (on < orec <= opar <= len(d)):
            c['x offsets not ordered'] += 1
            continue
        if orec - on != 32 * n:
            c['x name table != 32 x node count'] += 1
            continue
        c['name table == 32 x node count'] += 1
        names = [d[on + 32 * i:on + 32 * i + 32].split(b'\0')[0].decode('latin-1') for i in range(n)]
        sim = sum(x.startswith('newtonparticle') for x in names)
        for x in names:
            if not x.startswith('newtonparticle'):
                anchors[x] += 1
        c['nodes'] += n
        c['simulated nodes'] += sim
        rec = orec and (opar - orec)
        sizes[('record bytes / simulated', rec / sim if sim else None, 'a', a, 'b', b, 'n', n, 'sim', sim)] += 1
    for k, v in sorted(c.items()):
        print(f'{k:<45} {v:>9}')
    per = collections.Counter(k[1] for k in sizes.elements())
    print('record bytes per simulated node:', per.most_common(5))
    print('a == simulated - 1?', sum(v for k, v in sizes.items() if k[3] == k[9] - 1), 'b == a?', sum(v for k, v in sizes.items() if k[5] == k[3]))
    print('anchor bones:', anchors.most_common(15))


if __name__ == '__main__':
    main()
