"""gr2_tail -- decode push 3 (#429): frame the tail of every .gr2 (GAWB),
the bytes between the header's 0x0C offset and the 'EGCD' footer.

Layout tested on every file (little endian):
  u32 L                      BNRY collision blob length (0 = no blob)
  L bytes                    'BNRY' u32 0x02000000 'LTLE' ... (internals not decoded)
  with a blob: u32 n, n x (u32 len, len bytes) names (collision object name)
  without: 28 more zero bytes
  meshes x f32[6]            per-mesh bounds (min xyz, max xyz)
  u32 0
  'EGCD' u32 version u32 tail offset
Each mesh's f32[6] is checked against its vertices (every vertex inside).

Usage: python3 -I gr2_tail.py <dir of GAWB files>
"""
import collections
import os
import struct
import sys


def check(d, c):
    tail = struct.unpack_from('<I', d, 0x0C)[0]
    nm = struct.unpack_from('<H', d, 0x18)[0]
    o = tail
    L = struct.unpack_from('<I', d, o)[0]
    o += 4
    if L:
        if d[o:o + 4] != b'BNRY' or d[o + 8:o + 12] != b'LTLE':
            c['x blob without BNRY/LTLE'] += 1
            return
        c['BNRY blob'] += 1
        o += L
        n = struct.unpack_from('<I', d, o)[0]
        o += 4
        for _ in range(n):
            ln = struct.unpack_from('<I', d, o)[0]
            o += 4 + ln
        c[f'BNRY names {n}'] += 1
    else:
        if any(d[o:o + 28]):
            c['x no-BNRY tail: 28 bytes not zero'] += 1
        o += 28
        c['no BNRY: 32 zero bytes'] += 1
    mo = struct.unpack_from('<Q', d, 0x58)[0]
    inside = True
    for m in range(nm):
        bb = struct.unpack_from('<6f', d, o + 24 * m)
        h = mo + 64 * m
        vflags, vsize, nv = struct.unpack_from('<III', d, h + 16)
        vo = struct.unpack_from('<Q', d, h + 32)[0]
        if vflags & 1:
            for v in range(nv):
                p = struct.unpack_from('<3f', d, vo + v * vsize)
                if any(p[k] < bb[k] - 1e-3 or p[k] > bb[k + 3] + 1e-3 for k in range(3)):
                    inside = False
                    break
    c['mesh bounds hold their vertices' if inside else 'x a vertex outside its mesh bounds'] += 1
    o += 24 * nm
    if struct.unpack_from('<I', d, o)[0] != 0:
        c['x no zero u32 before footer'] += 1
    o += 4
    if d[o:o + 4] == b'EGCD' and o + 12 == len(d):
        c['tail framed to the footer'] += 1
    else:
        c['x tail does not reach the footer'] += 1


def main():
    src = sys.argv[1]
    c = collections.Counter()
    for fn in os.listdir(src):
        d = open(os.path.join(src, fn), 'rb').read()
        c['files'] += 1
        try:
            check(d, c)
        except struct.error:
            c['x short'] += 1
    for k, v in sorted(c.items()):
        print(f'{k:<50} {v:>9}')


if __name__ == '__main__':
    main()
