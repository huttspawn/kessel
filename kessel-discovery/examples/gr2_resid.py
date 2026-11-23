"""gr2_resid -- decode push 3 (#429): the residual GR2 (GAWB) checks that
gr2_census.rs leaves open, run over every model dumped by media_census.

  1. bone weights that do not sum to 255: what the sums are
  2. header bytes 0x40..0x50 that are not zero: what they hold
  3. normals: u8x4 decoded as b/127.5 - 1 have unit length
  4. bones (144 B): is the second matrix the inverse of the chained local
     matrices (world = parent world x local, row-vector convention)?
  5. file kind 1: does the model have a sibling .clo (cloth) file by path?

Usage: python3 -I gr2_resid.py <dir of GAWB files> <hashes_filename.txt>
"""
import collections
import os
import struct
import sys
import importlib.util

import numpy as np

here = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location('ap', os.path.join(here, 'appearance_paths.py'))
ap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ap)


def main():
    src, dictp = sys.argv[1], sys.argv[2]
    paths = {}
    clo = set()
    for line in open(dictp, errors='replace'):
        p = line.split('#')
        if len(p) < 3:
            continue
        h = (int(p[0], 16) << 32) | int(p[1], 16)
        if p[2].endswith('.gr2'):
            paths[h] = p[2]
        elif p[2].endswith('.clo'):
            clo.add(h)
    c = collections.Counter()
    wsums = collections.Counter()
    hdr = collections.Counter()
    ntot = nok = 0
    nhist = np.zeros(40, dtype=np.int64)
    berr = []
    for fn in os.listdir(src):
        d = open(os.path.join(src, fn), 'rb').read()
        h = int(fn[:16], 16)
        kind = struct.unpack_from('<I', d, 0x14)[0]
        nm, nmat, nbone = struct.unpack_from('<HHH', d, 0x18)
        mo, bo = struct.unpack_from('<Q', d, 0x58)[0], struct.unpack_from('<Q', d, 0x68)[0]
        if any(d[0x40:0x50]):
            hdr[(kind, d[0x40:0x50].hex())] += 1
        # 5. kind vs cloth sibling
        if h in paths:
            sib = ap.hashlittle2(paths[h][:-4].replace('.lod', '').encode() + b'.clo') in clo
            c[f'kind {kind}: sibling .clo {"yes" if sib else "no"}'] += 1
        for m in range(nm):
            mh = mo + 64 * m
            vf, vs, nv = struct.unpack_from('<III', d, mh + 16)
            vo = struct.unpack_from('<Q', d, mh + 32)[0]
            if not nv:
                continue
            raw = np.frombuffer(d, dtype=np.uint8, count=nv * vs, offset=vo).reshape(nv, vs)
            o = 12
            if vf & 0x100:
                w = raw[:, o:o + 4].astype(np.int32).sum(axis=1)
                bad = w[w != 255]
                for v in bad.tolist():
                    wsums[v] += 1
                o += 8
            if vf & 0x002:
                n = raw[:, o:o + 3].astype(np.float32) / 127.5 - 1.0
                ln = np.linalg.norm(n, axis=1)
                ntot += ln.size
                nok += int((np.abs(ln - 1) < 0.02).sum())
                nhist += np.histogram(ln, bins=40, range=(0, 2))[0]
        # 4. bones
        if nbone:
            world = []
            for b in range(nbone):
                bh = bo + 144 * b
                parent = struct.unpack_from('<i', d, bh + 8)[0]
                loc = np.array(struct.unpack_from('<16f', d, bh + 16), dtype=np.float64).reshape(4, 4)
                inv = np.array(struct.unpack_from('<16f', d, bh + 80), dtype=np.float64).reshape(4, 4)
                wm = loc if parent < 0 else loc @ world[parent]
                world.append(wm)
                berr.append(float(np.abs(wm @ inv - np.eye(4)).max()))
    print('weights not 255, by sum:', sorted(wsums.items(), key=lambda x: -x[1])[:10])
    print('header 0x40..0x50 nonzero, by (kind, bytes):', hdr.most_common(6), 'total', sum(hdr.values()))
    if ntot:
        print(f'normals: {ntot} vertices, |n| within 0.02 of 1: {100 * nok / ntot:.2f}%; histogram 0..2 by 0.05: {nhist.tolist()}')
    if berr:
        e = np.array(berr)
        print(f'bones: {e.size}, |world x inverse - I| max-entry median {np.median(e):.2e}, < 1e-3: {np.mean(e < 1e-3) * 100:.2f}%')
    for k, v in sorted(c.items()):
        print(f'{k:<40} {v:>8}')


if __name__ == '__main__':
    main()
