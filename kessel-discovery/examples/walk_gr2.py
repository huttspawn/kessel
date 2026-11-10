"""walk_gr2 -- read SWTOR GR2 ("GAWB") meshes for the walk-mesh decode (#429).

Layout after the SWTOR Slicers Blender add-on (io_scene_gr2/ops/import_gr2.py),
checked here on every model an area places. Imported by walk_mesh.py; run alone
it summarises a directory of .gr2 files:  python3 walk_gr2.py <dir of .gr2>

Header: u32 magic GAWB, u32 version (4; 5 = 64-bit offsets), u32 type at 0x14,
u16 mesh / material / bone counts at 0x18, 8 x f32 bounds at 0x30, offsets at
0x50 (cached offsets, mesh headers, material names, bones). Mesh header (40
bytes; 64 in v5): name offset, u32 flag1, u16 pieces, u16 used bones, u16 flag2,
u16 vertex size, u32 vertices, u32 faces, offsets of vertices, pieces, u16
index triples and bones. Piece (48 bytes): u32 first face, u32 faces, u32
material, u32 index, 8 x f32 bounds. A vertex starts with 3 x f32 position.
"""
import collections
import os
import struct
import sys

import numpy as np

GAWB = 0x42574147


def read_gr2(data):
    """Return {"version", "bounds", "materials", "meshes": [...]}; each mesh has
    name, flag1, flag2, vsize, n_vert, n_face, verts (N,3), tris (M,3), pieces."""
    magic, version = struct.unpack_from("<II", data, 0)
    if magic != GAWB:
        raise ValueError("not GAWB")
    v5 = version == 5
    n_mesh, n_mat, _n_bone = struct.unpack_from("<HHH", data, 0x18)
    bounds = struct.unpack_from("<8f", data, 0x30)
    if v5:
        off_mesh, off_mat = struct.unpack_from("<QQ", data, 0x58)
    else:
        off_mesh, off_mat = struct.unpack_from("<II", data, 0x54)

    def cstr(o):
        e = data.index(b"\0", o)
        return data[o:e].decode("latin1")

    mats = []
    for i in range(n_mat):
        o = struct.unpack_from("<Q" if v5 else "<I", data, off_mat + i * (8 if v5 else 4))[0]
        mats.append(cstr(o))
    meshes = []
    size = 64 if v5 else 40
    for i in range(n_mesh):
        p = off_mesh + i * size
        if v5:
            name_off = struct.unpack_from("<Q", data, p)[0]
            q = p + 8
        else:
            name_off = struct.unpack_from("<I", data, p)[0]
            q = p + 4
        flag1, n_piece, _n_used = struct.unpack_from("<IHH", data, q)
        q += 8
        if v5:
            flag2, vsize = struct.unpack_from("<II", data, q)
            q += 8
        else:
            flag2, vsize = struct.unpack_from("<HH", data, q)
            q += 4
        n_vert, n_face = struct.unpack_from("<II", data, q)
        q += 8
        if v5:
            o_vert, o_piece, o_idx, _o_bone = struct.unpack_from("<QQQQ", data, q)
        else:
            o_vert, o_piece, o_idx, _o_bone = struct.unpack_from("<IIII", data, q)
        raw = np.frombuffer(data, dtype=np.uint8, count=n_vert * vsize, offset=o_vert)
        raw = raw.reshape(n_vert, vsize)
        verts = raw[:, :12].copy().view("<f4").reshape(n_vert, 3)
        pieces = []
        total = 0
        for j in range(n_piece):
            po, pn, pm, _pi = struct.unpack_from("<IIII", data, o_piece + j * 48)
            mat = mats[pm] if pm < len(mats) else None
            pieces.append({"first": po, "faces": pn, "material": mat})
            total += pn
        tris = np.frombuffer(data, dtype="<u2", count=total * 3, offset=o_idx).reshape(total, 3)
        meshes.append({
            "name": cstr(name_off), "flag1": flag1, "flag2": flag2, "vsize": vsize,
            "n_vert": n_vert, "n_face": n_face, "verts": verts, "tris": tris, "pieces": pieces,
        })
    return {"version": version, "bounds": bounds, "materials": mats, "meshes": meshes}


def is_collision(mesh):
    return "collision" in mesh["name"].lower()


def main(d):
    stats = collections.Counter()
    kinds = collections.Counter()
    for f in sorted(os.listdir(d)):
        try:
            with open(os.path.join(d, f), "rb") as fh:
                g = read_gr2(fh.read())
        except (ValueError, struct.error, IndexError) as e:
            stats["fail"] += 1
            print("FAIL", f, e)
            continue
        stats["models"] += 1
        if any(is_collision(m) for m in g["meshes"]):
            stats["with_collision_mesh"] += 1
        for m in g["meshes"]:
            kinds[(is_collision(m), m["vsize"], m["flag2"])] += 1
            if m["n_face"] != len(m["tris"]):
                stats["face_count_ne_piece_sum"] += 1
            if len(m["tris"]) and m["tris"].max() >= m["n_vert"]:
                stats["index_out_of_range"] += 1
    print(dict(stats))
    print("(is_collision, vertex size, flag2): meshes")
    for k, v in sorted(kinds.items(), key=lambda x: -x[1]):
        print(" ", k, v)


if __name__ == "__main__":
    main(sys.argv[1])
