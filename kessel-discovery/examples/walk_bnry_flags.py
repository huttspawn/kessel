"""walk_bnry_flags -- do the cooked collision blob's per-triangle bytes mark walkability? (#429)

The GR2 tail's BNRY blob holds a cooked copy of the collision mesh (models
decoder, research/decode3-models). For collision meshes small enough for u8
vertex indices, its triangle records are 7 bytes: 0x21, three u8 vertex
indices, three flag bytes, directly after the cooked vertex list (f32 x3 per
vertex) and a u32. This finds that run, rebuilds each triangle from the cooked
vertices, and asks whether any bit of the three flag bytes, or the bytes as a
set, separates floors (normal within GroundSlopeMax 46 degrees of up) from walls.

Usage: python3 -I walk_bnry_flags.py <dir of .gr2>
"""
import collections
import math
import os
import struct
import sys

import numpy as np

from walk_gr2 import read_gr2, is_collision

COS = math.cos(math.radians(46))


def records(blob, n_tri, n_vert):
    """(cooked verts (k,3), records (n,7)) or None."""
    need = 7 * n_tri
    for end in range(len(blob) - 4, need - 1, -1):
        start = end - need
        rec = np.frombuffer(blob, np.uint8, need, start).reshape(n_tri, 7)
        if not (rec[:, 0] == 0x21).all():
            continue
        k = int(rec[:, 1:4].max()) + 1
        vstart = start - 4 - 12 * k
        if vstart < 0:
            continue
        verts = np.frombuffer(blob, "<f4", 3 * k, vstart).reshape(k, 3)
        if not np.isfinite(verts).all():
            continue
        return verts, rec
    return None


def main():
    d = sys.argv[1]
    bits = collections.Counter()  # (bit, floor) -> triangles with bit set
    floors = walls = 0
    byte_vals = collections.Counter()
    files = found = 0
    for fn in sorted(os.listdir(d)):
        data = open(os.path.join(d, fn), "rb").read()
        g = read_gr2(data)
        coll = [m for m in g["meshes"] if is_collision(m)]
        if not coll:
            continue
        files += 1
        m = coll[0]
        if m["n_vert"] > 255:
            continue
        tail = struct.unpack_from("<I", data, 0x0C)[0]
        L = struct.unpack_from("<I", data, tail)[0]
        if not L:
            continue
        r = records(data[tail + 4:tail + 4 + L], len(m["tris"]), m["n_vert"])
        if r is None:
            continue
        found += 1
        verts, rec = r
        T = verts[rec[:, 1:4].astype(int)]
        n = np.cross(T[:, 1] - T[:, 0], T[:, 2] - T[:, 0])
        ln = np.linalg.norm(n, axis=1)
        ok = ln > 1e-12
        floor = np.zeros(len(rec), bool)
        floor[ok] = np.abs(n[ok, 1]) / ln[ok] >= COS
        floors += int(floor.sum())
        walls += int((~floor).sum())
        for t in range(len(rec)):
            for j in range(3):
                byte_vals[(int(rec[t, 4 + j]), bool(floor[t]))] += 1
                for b in range(8):
                    if rec[t, 4 + j] >> b & 1:
                        bits[(j, b, bool(floor[t]))] += 1
    print(f"models with collision: {files}; u8-index blobs framed: {found}; floor tris {floors}, wall tris {walls}")
    print("byte slot, bit: share of floor tris with it set vs share of wall tris")
    for j in range(3):
        for b in range(8):
            f = bits[(j, b, True)] / max(floors, 1)
            w = bits[(j, b, False)] / max(walls, 1)
            print(f"  slot {j} bit {b}: floor {f:.3f}  wall {w:.3f}")
    vals = sorted({v for v, _ in byte_vals})
    top = sorted(vals, key=lambda v: -(byte_vals[(v, True)] + byte_vals[(v, False)]))[:15]
    print("commonest byte values: value floor wall")
    for v in top:
        print(f"  {v:#04x} {byte_vals[(v, True)]} {byte_vals[(v, False)]}")


if __name__ == "__main__":
    main()
