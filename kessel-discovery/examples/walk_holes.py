"""walk_holes -- settle the heightmap hole bitmap's bit order and polarity (#429).

For every terrain patch with a hole bitmap, take every point that stands on that
patch's terrain (a creature/placeable spawn from spice placements, or a player
position from the combat logs, within 0.1 units of the terrain height at the
nearest grid point). A point standing on terrain cannot stand in a hole, so
under the right reading of the bitmap almost every such point falls on a
non-hole grid point. Four readings are scored: bit order row-major with x
fastest ("z") or with z fastest ("x", as the inner heights are stored), and a
set bit meaning hole or meaning terrain.

Usage: python3 walk_holes.py <extract dir> <area dir id> <spice.sqlite> <logs dir>
"""
import json
import sys

import numpy as np

import walk_mesh as W


def main():
    ext, area, spice, logs = sys.argv[1:5]
    insts = W.load_instances(ext)
    xf = W.world_transforms(insts, "zxy")
    import sqlite3
    db = sqlite3.connect(f"file:{spice}?mode=ro", uri=True)
    pts = [("spawn", x, y, z) for x, y, z in db.execute(
        "SELECT world_x, world_y, world_z FROM placements WHERE area_dir_id=?", (int(area),))]
    log_id = db.execute("SELECT log_area_id FROM area_log_ids WHERE area_dir_id=? LIMIT 1",
                        (int(area),)).fetchone()
    if log_id:
        pts += [("log", x, h, z) for x, h, z in W.log_positions(logs, log_id[0])]
    P = np.array([[p[1], p[2], p[3]] for p in pts])
    kind = np.array([p[0] for p in pts])
    score = {f"{o}_{pol}": {"on_terrain": 0, "in_hole": 0} for o in "zx" for pol in ("set_is_hole", "set_is_terrain")}
    patches = 0
    hole_frac = []
    examples = []
    for iid, o in insts.items():
        if W.HEIGHTMAP not in o["props"]:
            continue
        import os
        raw = W.decompress(open(os.path.join(ext, "blob", f"{iid}_a3ab26ae.bin"), "rb").read())
        flags = raw[0]
        w, d = int.from_bytes(raw[1:5], "little"), int.from_bytes(raw[5:9], "little")
        p = 9 + (12 if flags & 8 else 0)
        if not raw[p]:
            continue
        n = (w * d + 7) >> 3
        bits = np.unpackbits(np.frombuffer(raw, np.uint8, n, p + 1), bitorder="little")[: w * d].astype(bool)
        _, _, h, _ = W.decode_heights(raw)
        patches += 1
        hole_frac.append(float(bits.mean()))
        pos = xf[iid][0]
        gx0 = pos[0] - W.SPACING * 0.5 * (w - 1) - (W.SPACING / 2 if w % 2 == 0 else 0)
        gz0 = pos[2] - W.SPACING * 0.5 * (d - 1) - (W.SPACING / 2 if d % 2 == 0 else 0)
        i = np.round((P[:, 0] - gx0) / W.SPACING).astype(int)
        j = np.round((P[:, 2] - gz0) / W.SPACING).astype(int)
        inside = (i >= 0) & (i < w) & (j >= 0) & (j < d)
        for k in np.flatnonzero(inside):
            th = h[j[k], i[k]] + pos[1]
            if abs(P[k, 1] - th) > 0.1:
                continue
            bz = bits.reshape(d, w)[j[k], i[k]]
            bx = bits.reshape(w, d).T[j[k], i[k]]
            for order, b in (("z", bz), ("x", bx)):
                for pol, hole in (("set_is_hole", b), ("set_is_terrain", not b)):
                    s = score[f"{order}_{pol}"]
                    s["on_terrain"] += 1
                    s["in_hole"] += int(hole)
            if len(examples) < 10:
                examples.append({"kind": str(kind[k]), "world": [round(float(v), 2) for v in P[k]],
                                 "patch": iid, "room": o["room"], "bit_z": int(bz), "bit_x": int(bx)})
    print(json.dumps({"patches_with_holes": patches,
                      "set_bit_fraction": {"min": min(hole_frac), "median": float(np.median(hole_frac)),
                                           "max": max(hole_frac)} if hole_frac else None,
                      "points": {"spawn": int((kind == "spawn").sum()), "log": int((kind == "log").sum())},
                      "readings": score, "examples": examples}, indent=1))


if __name__ == "__main__":
    main()
