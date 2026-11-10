"""walk_mesh -- draw an area's walkable and unwalkable surface from the client (#429).

Input is what `walk_extract area` writes for one area (instances.jsonl, blob/,
gr2/). The client holds no navmesh; it holds the two surfaces a character
stands on and is stopped by:

  * terrain: each `\\engine\\heightmap.hms` instance carries a heightmap blob
    (property a3ab26ae) with a hole bitmap; a hole removes the terrain there.
  * collision meshes: almost every placed `.gr2` model carries a mesh named
    `generated_collision` (vertex size 12, flag2 1, position only), in the
    model's own frame. Placed by the instance transform (position 4f77e269,
    Euler degrees 8c64af1e composed R = Ry Rx Rz, scale b181622a, parent
    40865e7f), it is the model's solid shape in the world.

Walkability is derived, as a character controller does: a surface is a floor
where its upward normal is within the slope limit, and a floor is standable
where no solid lies between step height and head height above it. Cells are
0.05 world units (0.5 m). The drawing is in the map frame: world x to the
right, world z down the page, as the game's map pages are laid out (x, z).

Usage:
  python3 walk_mesh.py <extract dir> <area dir id> <spice.sqlite> <logs dir>
         <x0> <z0> <x1> <z1> <out.png> [--holes set|clear] [--slope DEG]
         [--rot zxy|xyz|yxz] [--report out.json]
"""
import argparse
import json
import math
import os
import re
import sqlite3
import struct
import zlib
import gzip

import numpy as np
from PIL import Image, ImageDraw

from walk_gr2 import read_gr2, is_collision

POS, ROT, SCALE, PARENT = "6:4f77e269", "6:8c64af1e", "6:b181622a", "5:40865e7f"
HEIGHTMAP = "9:a3ab26ae"
SPACING = 0.2
CELL = 0.05
STEP = 0.05  # 0.5 m: solids this close above a floor are stepped over
HEAD = 0.20  # 2 m: a floor needs this much clear space above it


def decompress(b):
    tag = struct.unpack_from("<H", b)[0]
    if tag & 0xFF == 0x78:
        return zlib.decompress(b)
    if tag == 0x8B1F:
        return gzip.decompress(b)
    if tag == 0xB528:
        from compression import zstd
        return zstd.decompress(b)
    raise ValueError(f"heightmap compression {tag:04x}")


def decode_heights(raw):
    """(w, d, heights (d, w) relative to the instance, hole bits (d, w) or None)."""
    p = 0
    flags = raw[p]
    p += 1
    w, d = struct.unpack_from("<II", raw, p)
    p += 8
    if flags & 8:
        p += 12
    holes = None
    if raw[p]:
        n = (w * d + 7) >> 3
        bits = np.unpackbits(np.frombuffer(raw, np.uint8, n, p + 1), bitorder="little")
        holes = bits[: w * d].reshape(d, w).astype(bool)
        p += n
    p += 1
    h = np.zeros((d, w), np.float32)
    top_bottom = np.frombuffer(raw, "<f4", 2 * w, p).reshape(w, 2)
    p += 8 * w
    h[0, :], h[d - 1, :] = top_bottom[:, 0], top_bottom[:, 1]
    sides = np.frombuffer(raw, "<f4", 2 * (d - 2), p).reshape(d - 2, 2)
    p += 8 * (d - 2)
    h[1:-1, 0], h[1:-1, w - 1] = sides[:, 0], sides[:, 1]
    inner = np.frombuffer(raw, "<u2", (w - 2) * (d - 2), p).reshape(w - 2, d - 2)
    h[1:-1, 1:-1] = inner.T * 0.0019531548
    return w, d, h, holes


def rot_matrix(deg, order):
    rx, ry, rz = (math.radians(a) for a in deg)
    cx, sx, cy, sy, cz, sz = math.cos(rx), math.sin(rx), math.cos(ry), math.sin(ry), math.cos(rz), math.sin(rz)
    X = np.array([[1, 0, 0], [0, cx, -sx], [0, sx, cx]])
    Y = np.array([[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]])
    Z = np.array([[cz, -sz, 0], [sz, cz, 0], [0, 0, 1]])
    return {"zxy": Y @ X @ Z, "xyz": Z @ Y @ X, "yxz": Z @ X @ Y}[order]


def load_instances(ext_dir):
    out = {}
    with open(os.path.join(ext_dir, "instances.jsonl")) as f:
        for line in f:
            o = json.loads(line)
            out[o["id"]] = o
    return out


def world_transforms(insts, order):
    memo = {}

    def resolve(iid, depth=0):
        if iid in memo:
            return memo[iid]
        p = insts[iid]["props"]
        pos = np.array(p.get(POS, [0, 0, 0]), float)
        m = rot_matrix(p.get(ROT, [0, 0, 0]), order)
        sc = np.array(p.get(SCALE, [1, 1, 1]), float)
        parent = p.get(PARENT, "0")
        if parent != "0" and parent in insts and depth < 32:
            ppos, pm, psc = resolve(parent, depth + 1)
            pos = ppos + pm @ (pos * psc)
            m = pm @ m
            sc = sc * psc
        memo[iid] = (pos, m, sc)
        return memo[iid]

    return {i: resolve(i) for i in insts}


def terrain_tris(ext_dir, insts, xf, box, hole_bit_means):
    """World triangles (N,3,3) of every heightmap inside box, holes removed."""
    x0, z0, x1, z1 = box
    tris = []
    stats = {"patches": 0, "hole_points": 0, "points": 0}
    for iid, o in insts.items():
        if HEIGHTMAP not in o["props"]:
            continue
        raw = decompress(open(os.path.join(ext_dir, "blob", f"{iid}_a3ab26ae.bin"), "rb").read())
        w, d, h, holes = decode_heights(raw)
        pos, _m, _sc = xf[iid]
        gx0 = pos[0] - SPACING * 0.5 * (w - 1) - (SPACING / 2 if w % 2 == 0 else 0)
        gz0 = pos[2] - SPACING * 0.5 * (d - 1) - (SPACING / 2 if d % 2 == 0 else 0)
        if gx0 > x1 or gz0 > z1 or gx0 + SPACING * w < x0 or gz0 + SPACING * d < z0:
            continue
        stats["patches"] += 1
        stats["points"] += w * d
        solid = np.ones((d, w), bool)
        if holes is not None:
            stats["hole_points"] += int(holes.sum())
            solid = ~holes if hole_bit_means == "set" else holes
        xs = gx0 + SPACING * np.arange(w)
        zs = gz0 + SPACING * np.arange(d)
        X, Z = np.meshgrid(xs, zs)
        H = h + pos[1]
        P = np.stack([X, H, Z], -1)
        # A quad is solid when all four corners are.
        q = solid[:-1, :-1] & solid[1:, :-1] & solid[:-1, 1:] & solid[1:, 1:]
        a, b, c, e = P[:-1, :-1][q], P[:-1, 1:][q], P[1:, :-1][q], P[1:, 1:][q]
        tris.append(np.stack([a, c, b], 1))
        tris.append(np.stack([b, c, e], 1))
    return (np.concatenate(tris) if tris else np.zeros((0, 3, 3))), stats


def collision_tris(ext_dir, insts, xf, box, use_render_without_collision):
    x0, z0, x1, z1 = box
    cache = {}
    tris = []
    stats = {"model_instances": 0, "with_collision": 0, "without_collision": 0, "no_file": 0}
    for iid, o in insts.items():
        path = o.get("path") or ""
        if not path.endswith(".gr2"):
            continue
        pos, m, sc = xf[iid]
        f = os.path.join(ext_dir, "gr2", f"{o['asset']}.gr2")
        if not os.path.exists(f):
            stats["no_file"] += 1
            continue
        if o["asset"] not in cache:
            g = read_gr2(open(f, "rb").read())
            coll = [mm for mm in g["meshes"] if is_collision(mm)]
            if not coll and use_render_without_collision:
                coll = g["meshes"]
            cache[o["asset"]] = (bool([mm for mm in g["meshes"] if is_collision(mm)]),
                                 [mm["verts"][mm["tris"]] for mm in coll if len(mm["tris"])])
        has, parts = cache[o["asset"]]
        hit = False
        for t in parts:
            wt = (t * sc) @ m.T + pos
            if (wt[:, :, 0].max() < x0 or wt[:, :, 0].min() > x1
                    or wt[:, :, 2].max() < z0 or wt[:, :, 2].min() > z1):
                continue
            hit = True
            tris.append(wt)
        if hit or not parts:
            if not parts and not (x0 <= pos[0] <= x1 and z0 <= pos[2] <= z1):
                continue
            stats["model_instances"] += 1
            stats["with_collision" if has else "without_collision"] += 1
    return (np.concatenate(tris) if tris else np.zeros((0, 3, 3))), stats


def sample_tris(tris, box, slope_deg):
    """Point samples (cell index, height, is_floor) covering every triangle."""
    x0, z0, x1, z1 = box
    nx, nz = int(round((x1 - x0) / CELL)), int(round((z1 - z0) / CELL))
    keep = ((tris[:, :, 0].max(1) >= x0) & (tris[:, :, 0].min(1) <= x1)
            & (tris[:, :, 2].max(1) >= z0) & (tris[:, :, 2].min(1) <= z1))
    tris = tris[keep]
    n = np.cross(tris[:, 1] - tris[:, 0], tris[:, 2] - tris[:, 0])
    ln = np.linalg.norm(n, axis=1)
    ok = ln > 1e-12
    tris, n, ln = tris[ok], n[ok], ln[ok]
    ny = np.abs(n[:, 1]) / ln  # winding is not trusted; a floor faces up or down
    floor = ny >= math.cos(math.radians(slope_deg))
    ext = np.maximum(np.ptp(tris[:, :, 0], 1), np.ptp(tris[:, :, 2], 1))
    k = np.clip(np.ceil(ext / CELL * 2).astype(int), 1, 400)
    cells, hs, fl = [], [], []
    for kk in np.unique(k):
        sel = k == kk
        T = tris[sel]
        i, j = np.meshgrid(np.arange(kk + 1), np.arange(kk + 1))
        mask = (i + j) <= kk
        u, v = (i[mask] / kk), (j[mask] / kk)
        P = (T[:, None, 0] * (1 - u - v)[None, :, None] + T[:, None, 1] * u[None, :, None]
             + T[:, None, 2] * v[None, :, None])
        cx = np.floor((P[..., 0] - x0) / CELL).astype(np.int64)
        cz = np.floor((P[..., 2] - z0) / CELL).astype(np.int64)
        inside = (cx >= 0) & (cx < nx) & (cz >= 0) & (cz < nz)
        cells.append((cz * nx + cx)[inside])
        hs.append(P[..., 1][inside])
        fl.append(np.broadcast_to(floor[sel][:, None], inside.shape)[inside])
    if not cells:
        return nx, nz, np.zeros(0, np.int64), np.zeros(0), np.zeros(0, bool)
    return nx, nz, np.concatenate(cells), np.concatenate(hs), np.concatenate(fl)


def classify(nx, nz, cells, hs, fl):
    """Per cell: top standable floor height (nan if none), top surface height, any wall."""
    order = np.lexsort((hs, cells))
    cells, hs, fl = cells[order], hs[order], fl[order]
    # Quantise heights to 1 cm so repeated samples of one surface collapse.
    q = np.round(hs, 2)
    key = np.unique(np.stack([cells, (q * 100).astype(np.int64), fl.astype(np.int64)], 1), axis=0)
    cells, hs, fl = key[:, 0], key[:, 1] / 100.0, key[:, 2].astype(bool)
    stand = np.full(nx * nz, np.nan)
    floors_all = {}
    top = np.full(nx * nz, np.nan)
    wall = np.zeros(nx * nz, bool)
    starts = np.flatnonzero(np.r_[True, cells[1:] != cells[:-1]])
    ends = np.r_[starts[1:], len(cells)]
    for s, e in zip(starts, ends):
        c = cells[s]
        H, F = hs[s:e], fl[s:e]
        top[c] = H[-1]
        if (~F).any():
            wall[c] = True
        good = []
        for idx in np.flatnonzero(F):
            h = H[idx]
            above = H[(H > h + STEP) & (H < h + HEAD)]
            if above.size == 0:
                good.append(h)
        if good:
            stand[c] = good[-1]
            floors_all[c] = good
    return stand, top, wall, floors_all


def log_positions(logs_dir, log_area_id):
    """Player positions (world x, height, z) while in the area, from combat logs."""
    pat = re.compile(r"^\[[^\]]+\] \[@[^#|/\]]+#\d+\|\((-?[\d.]+),(-?[\d.]+),(-?[\d.]+),")
    out = []
    for fn in sorted(os.listdir(logs_dir)):
        inside = False
        with open(os.path.join(logs_dir, fn), errors="replace") as f:
            for line in f:
                if "AreaEntered" in line:
                    inside = f"{{{log_area_id}}}" in line
                if not inside:
                    continue
                m = pat.match(line)
                if m:
                    a, b, c = (float(v) for v in m.groups())
                    if a == 0 and b == 0 and c == 0:
                        continue
                    out.append((a / 10, c / 10, b / 10))
    return sorted(set(out))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("ext")
    ap.add_argument("area")
    ap.add_argument("spice")
    ap.add_argument("logs")
    ap.add_argument("box", nargs=4, type=float)
    ap.add_argument("out")
    ap.add_argument("--holes", default="set")
    ap.add_argument("--slope", type=float, default=45.0)
    ap.add_argument("--rot", default="zxy")
    ap.add_argument("--render-fallback", action="store_true")
    ap.add_argument("--report")
    a = ap.parse_args()
    box = a.box
    insts = load_instances(a.ext)
    xf = world_transforms(insts, a.rot)
    tt, tstats = terrain_tris(a.ext, insts, xf, box, a.holes)
    ct, cstats = collision_tris(a.ext, insts, xf, box, a.render_fallback)
    nx, nz, cells, hs, fl = sample_tris(np.concatenate([tt, ct]), box, a.slope)
    stand, top, wall, floors = classify(nx, nz, cells, hs, fl)
    _, _, tc, th, _ = sample_tris(tt, box, 90.0)
    terrain_cell = np.zeros(nx * nz, bool)
    terrain_cell[tc] = True

    db = sqlite3.connect(f"file:{a.spice}?mode=ro", uri=True)
    log_id = db.execute("SELECT log_area_id FROM area_log_ids WHERE area_dir_id=? LIMIT 1",
                        (int(a.area),)).fetchone()
    places = db.execute(
        "SELECT world_x, world_y, world_z, spawner_type, asset_path FROM placements WHERE area_dir_id=? "
        "AND world_x BETWEEN ? AND ? AND world_z BETWEEN ? AND ?",
        (int(a.area), box[0], box[2], box[1], box[3])).fetchall()
    logs = [p for p in log_positions(a.logs, log_id[0]) if box[0] <= p[0] < box[2] and box[1] <= p[2] < box[3]] \
        if log_id else []

    def nearest_floor(x, h, z, radius=1):
        cx, cz = int((x - box[0]) / CELL), int((z - box[1]) / CELL)
        best = None
        for dz in range(-radius, radius + 1):
            for dx in range(-radius, radius + 1):
                c = (cz + dz) * nx + (cx + dx)
                if 0 <= cx + dx < nx and 0 <= cz + dz < nz:
                    for f in floors.get(c, []):
                        d = h - f
                        if best is None or abs(d) < abs(best):
                            best = d
        return best

    def score(points):
        res = [nearest_floor(x, h, z) for x, h, z in points]
        on = [r for r in res if r is not None and -0.05 <= r <= 0.1]
        return {"n": len(points), "on_standable_floor": len(on),
                "no_floor_within_0.05": len(points) - len(on),
                "median_abs_dz": float(np.median([abs(r) for r in res if r is not None])) if any(
                    r is not None for r in res) else None}

    creature = [(x, y, z) for x, y, z, t, _ in places if t == "creature"]
    report = {
        "box": box, "cell": CELL, "slope_deg": a.slope, "step": STEP, "head": HEAD,
        "holes_bit": a.holes, "rotation": a.rot, "terrain": tstats, "collision": cstats,
        "terrain_tris": int(len(tt)), "collision_tris": int(len(ct)),
        "cells": nx * nz, "standable_cells": int(np.isfinite(stand).sum()),
        "log_positions": score(logs), "creature_spawns": score(creature),
        "all_placements": len(places),
    }

    # Drawing: standable floor shaded by height; unwalkable dark; walls red.
    img = np.zeros((nz, nx, 3), np.uint8)
    img[:] = (24, 24, 32)  # nothing to stand on (terrain hole with nothing below, void)
    s = stand.reshape(nz, nx)
    fin = np.isfinite(s)
    if fin.any():
        lo, hi = np.nanpercentile(s, 2), np.nanpercentile(s, 98)
        t = np.clip((s - lo) / max(hi - lo, 1e-6), 0, 1)
        g = (110 + 120 * t).astype(np.uint8)
        img[fin] = np.stack([g[fin] // 2 + 40, g[fin], g[fin] // 2 + 30], -1)
    blocked = (~fin) & np.isfinite(top.reshape(nz, nx))
    img[blocked] = (150, 60, 50)  # a surface is here but nobody can stand on it
    wl = wall.reshape(nz, nx) & ~fin
    img[wl] = (200, 70, 50)
    hole = (~terrain_cell.reshape(nz, nx)) & (~fin) & ~blocked
    img[hole] = (24, 24, 32)
    scale = 1
    im = Image.fromarray(img).resize((nx * scale, nz * scale), Image.NEAREST)
    dr = ImageDraw.Draw(im)

    def px(x, z):
        return (x - box[0]) / CELL * scale, (z - box[1]) / CELL * scale

    for x, y, z, t, ap_ in places:
        u, v = px(x, z)
        col = {"creature": (255, 220, 0), "placeable": (0, 200, 255)}.get(t, (255, 255, 255))
        dr.ellipse([u - 3, v - 3, u + 3, v + 3], outline=col, width=2)
    for x, h, z in logs:
        u, v = px(x, z)
        dr.line([u - 4, v, u + 4, v], fill=(255, 0, 255), width=2)
        dr.line([u, v - 4, u, v + 4], fill=(255, 0, 255), width=2)
    # Grid every 10 world units with labels, so the frame can be read off the image.
    for gx in range(int(math.ceil(box[0] / 10) * 10), int(box[2]) + 1, 10):
        u, _ = px(gx, box[1])
        dr.line([u, 0, u, nz * scale], fill=(90, 90, 110), width=1)
        dr.text((u + 2, 2), f"x={gx}", fill=(230, 230, 230))
    for gz in range(int(math.ceil(box[1] / 10) * 10), int(box[3]) + 1, 10):
        _, v = px(box[0], gz)
        dr.line([0, v, nx * scale, v], fill=(90, 90, 110), width=1)
        dr.text((2, v + 2), f"z={gz}", fill=(230, 230, 230))
    legend = [((90, 170, 85), "standable floor (lighter = higher)"), ((150, 60, 50), "surface, not standable"),
              ((200, 70, 50), "wall / steep"), ((24, 24, 32), "nothing (terrain hole, void)"),
              ((255, 220, 0), "creature spawn (placements)"), ((0, 200, 255), "placeable spawn"),
              ((255, 0, 255), "player position (combat log / 10)")]
    y = nz * scale - 16 * len(legend) - 6
    dr.rectangle([4, y - 4, 300, nz * scale - 4], fill=(0, 0, 0))
    for col, label in legend:
        dr.rectangle([8, y, 20, y + 10], fill=col)
        dr.text((26, y), label, fill=(255, 255, 255))
        y += 16
    im.save(a.out)
    if a.report:
        with open(a.report, "w") as f:
            json.dump(report, f, indent=1)
    print(json.dumps(report, indent=1))


if __name__ == "__main__":
    main()
