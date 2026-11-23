"""walk_rotation -- settle the Euler order of room instance rotations (#429).

Test 2 (--rest): boulders and logs rest on the terrain. For each instance
tilted over 20 degrees on both x and z, place its collision mesh under each
order and measure the lowest gap between its vertices and the terrain under
them; under a wrong order more of them float clear of the ground or sink whole.

Modular art meets edge to edge: a placed piece's collision vertices land on
the vertices of the pieces beside it. For every model instance tilted on both
x and z (rx and rz over 1 degree; only there does the order matter), its
collision-mesh vertices are placed under each candidate order and counted
when they coincide (within 0.002 units, 2 cm) with a vertex of another
instance's collision mesh placed under the same order. The right order makes
the most tilted vertices meet their neighbours.

Usage: python3 walk_rotation.py <extract dir>
"""
import json
import os
import sys

import numpy as np

import walk_mesh as W
from walk_gr2 import read_gr2, is_collision

Q = 0.002


def keys(v):
    q = np.round(v / Q).astype(np.int64) + (1 << 20)
    return (q[:, 0] << 42) | (q[:, 1] << 21) | q[:, 2]


def main():
    ext = sys.argv[1]
    insts = W.load_instances(ext)
    meshes = {}
    for o in insts.values():
        if (o.get("path") or "").endswith(".gr2") and o["asset"] not in meshes:
            f = os.path.join(ext, "gr2", f"{o['asset']}.gr2")
            v = None
            if os.path.exists(f):
                cs = [m["verts"] for m in read_gr2(open(f, "rb").read())["meshes"] if is_collision(m)]
                if cs:
                    v = np.unique(np.concatenate(cs), axis=0)
            meshes[o["asset"]] = v
    out = {}
    for order in ("zxy", "xyz", "yxz", "xzy", "yzx", "zyx"):
        W_rot = order
        xf = world_transforms_any(insts, W_rot)
        tilted, others = [], []
        for iid, o in insts.items():
            v = meshes.get(o.get("asset")) if (o.get("path") or "").endswith(".gr2") else None
            if v is None:
                continue
            r = o["props"].get(W.ROT, [0, 0, 0])
            pos, m, sc = xf[iid]
            wv = (v * sc) @ m.T + pos
            k = np.unique(keys(wv))
            if abs(r[0]) > 1 and abs(r[2]) > 1:
                tilted.append((iid, k))
            else:
                others.append(k)
        allk = np.concatenate([k for _, k in tilted] + others)
        uniq, counts = np.unique(allk, return_counts=True)
        total = met = 0
        for _, k in tilted:
            c = counts[np.searchsorted(uniq, k)]
            total += len(k)
            met += int((c > 1).sum())  # another instance has a vertex here too
        out[order] = {"tilted_instances": len(tilted), "tilted_vertices": total,
                      "met_a_neighbour": met, "fraction": round(met / max(total, 1), 4)}
        print(order, out[order], flush=True)
    print(json.dumps(out, indent=1))


def rot(deg, order):
    import math
    rx, ry, rz = (math.radians(a) for a in deg)
    cx, sx, cy, sy, cz, sz = math.cos(rx), math.sin(rx), math.cos(ry), math.sin(ry), math.cos(rz), math.sin(rz)
    M = {"x": np.array([[1, 0, 0], [0, cx, -sx], [0, sx, cx]]),
         "y": np.array([[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]]),
         "z": np.array([[cz, -sz, 0], [sz, cz, 0], [0, 0, 1]])}
    # order names the axis applied first: "zxy" = Z, then X, then Y = Ry Rx Rz.
    a, b, c = order
    return M[c] @ M[b] @ M[a]


def world_transforms_any(insts, order):
    memo = {}

    def resolve(iid, depth=0):
        if iid in memo:
            return memo[iid]
        p = insts[iid]["props"]
        pos = np.array(p.get(W.POS, [0, 0, 0]), float)
        m = rot(p.get(W.ROT, [0, 0, 0]), order)
        sc = np.array(p.get(W.SCALE, [1, 1, 1]), float)
        parent = p.get(W.PARENT, "0")
        if parent != "0" and parent in insts and depth < 32:
            ppos, pm, psc = resolve(parent, depth + 1)
            pos = ppos + pm @ (pos * psc)
            m = pm @ m
            sc = sc * psc
        memo[iid] = (pos, m, sc)
        return memo[iid]

    return {i: resolve(i) for i in insts}


def rest():
    ext = sys.argv[2]
    insts = W.load_instances(ext)
    xf0 = W.world_transforms(insts, "zxy")
    grids = []
    for iid, o in insts.items():
        if W.HEIGHTMAP not in o["props"]:
            continue
        raw = W.decompress(open(os.path.join(ext, "blob", f"{iid}_a3ab26ae.bin"), "rb").read())
        w, d, h, holes = W.decode_heights(raw)
        pos = xf0[iid][0]
        gx0 = pos[0] - W.SPACING * 0.5 * (w - 1) - (W.SPACING / 2 if w % 2 == 0 else 0)
        gz0 = pos[2] - W.SPACING * 0.5 * (d - 1) - (W.SPACING / 2 if d % 2 == 0 else 0)
        H = h + pos[1]
        if holes is not None:
            H = np.where(holes, H, np.nan)
        grids.append((gx0, gz0, w, d, H))

    def ground(x, y, z):
        best = np.full(len(x), np.nan)
        for gx0, gz0, w, d, H in grids:
            i = np.round((x - gx0) / W.SPACING).astype(int)
            j = np.round((z - gz0) / W.SPACING).astype(int)
            ok = (i >= 0) & (i < w) & (j >= 0) & (j < d)
            g = np.full(len(x), np.nan)
            g[ok] = H[j[ok], i[ok]]
            take = np.isfinite(g) & (~np.isfinite(best) | (np.abs(g - y) < np.abs(best - y)))
            best[take] = g[take]
        return best

    sel = [iid for iid, o in insts.items() if (o.get("path") or "").endswith(".gr2")
           and abs(o["props"].get(W.ROT, [0, 0, 0])[0]) > 20 and abs(o["props"].get(W.ROT, [0, 0, 0])[2]) > 20
           and o["props"].get(W.PARENT, "0") == "0"]
    cache = {}
    out = {}
    for order in ("zxy", "xyz", "yxz", "xzy", "yzx", "zyx"):
        res = {"instances": 0, "touching": 0, "floating": 0, "buried": 0, "no_ground": 0}
        for iid in sel:
            o = insts[iid]
            if o["asset"] not in cache:
                f = os.path.join(ext, "gr2", f"{o['asset']}.gr2")
                cs = [m["verts"] for m in read_gr2(open(f, "rb").read())["meshes"] if is_collision(m)] \
                    if os.path.exists(f) else []
                cache[o["asset"]] = np.concatenate(cs) if cs else None
            v = cache[o["asset"]]
            if v is None:
                continue
            p = o["props"]
            m = rot(p.get(W.ROT), order)
            wv = (v * np.array(p.get(W.SCALE, [1, 1, 1]))) @ m.T + np.array(p.get(W.POS))
            g = ground(wv[:, 0], wv[:, 1], wv[:, 2])
            ok = np.isfinite(g)
            res["instances"] += 1
            if not ok.any():
                res["no_ground"] += 1
                continue
            gap = wv[ok, 1] - g[ok]
            if gap.min() > 0.02:
                res["floating"] += 1
            elif gap.max() < 0:
                res["buried"] += 1
            else:
                res["touching"] += 1
        out[order] = res
        print(order, res, flush=True)


def joints():
    """Test 3 (--joints): a child placed on a parent model (parent 40865e7f) often
    joins it, so the child's vertices land on the parent's. In the parent's own
    frame only the child's rotation matters, so each convention (the six Euler
    orders, with the angles as stored or negated) is scored by how many of the
    child's vertices fall within 3 mm of a parent vertex, over every
    model-on-model pair whose child is turned on two or more axes."""
    import collections
    tally = collections.defaultdict(lambda: [0, 0])  # convention -> [pairs best, vertices met]
    pairs = 0
    for ext in sys.argv[2:]:
        insts = W.load_instances(ext)
        cache = {}

        def verts(a):
            if a not in cache:
                f = os.path.join(ext, "gr2", f"{a}.gr2")
                g = read_gr2(open(f, "rb").read()) if os.path.exists(f) else None
                cache[a] = np.concatenate([m["verts"] for m in g["meshes"]]) if g else None
            return cache[a]

        for iid, o in insts.items():
            p = o["props"]
            par = p.get(W.PARENT, "0")
            if par == "0" or par not in insts or not (o.get("path") or "").endswith(".gr2"):
                continue
            if not (insts[par].get("path") or "").endswith(".gr2"):
                continue
            r = p.get(W.ROT, [0, 0, 0])
            if sum(abs(a) > 1 for a in r) < 2:
                continue
            A, B = verts(o["asset"]), verts(insts[par]["asset"])
            if A is None or B is None:
                continue
            A = A * np.array(p.get(W.SCALE, [1, 1, 1]))
            lo, hi = None, None
            score = {}
            for order in ("zxy", "xyz", "yxz", "xzy", "yzx", "zyx"):
                for sign in (1, -1):
                    m = rot([sign * a for a in r], order)
                    C = A @ m.T + np.array(p.get(W.POS, [0, 0, 0]))
                    lo, hi = C.min(0) - 0.003, C.max(0) + 0.003
                    Bn = B[np.all((B >= lo) & (B <= hi), 1)]
                    met = 0
                    if len(Bn):
                        d = np.full(len(C), 9.0)
                        for s0 in range(0, len(Bn), 4000):
                            d = np.minimum(d, np.sqrt(((C[:, None] - Bn[None, s0:s0 + 4000]) ** 2).sum(-1)).min(1))
                        met = int((d < 0.003).sum())
                    score[f"{order}{'+' if sign > 0 else '-'}"] = met
            best = max(score.values())
            if best < 3:
                continue
            pairs += 1
            for k, v in score.items():
                tally[k][1] += v
                tally[k][0] += int(v == best)
    print("pairs with a joint under some convention:", pairs)
    for k, (b, v) in sorted(tally.items(), key=lambda kv: -kv[1][0]):
        print(f"  {k}: best on {b} pairs, {v} vertices met")


if __name__ == "__main__":
    if sys.argv[1] == "--joints":
        joints()
    elif sys.argv[1] == "--rest":
        rest()
    else:
        main()
