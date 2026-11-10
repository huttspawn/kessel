"""jawb_census -- decode push 3 (#429), media group: walk every JAWB v2
animation (.jba, magic 'JAWB') to its last byte, and join it to the AMX
network that names it.

Layout (little endian), first decoded 2026-10-20 on 142 saber animations
(reflection 01a15add, reader ~/ml/huttspawn-art/anim/jawb.py), checked here on
every file:
  0x00 'JAWB' u32 version 2, u64 0
  0x10 f32 length (s)  0x14 f32 fps  0x18 u32 block count  0x30 u32 track count
  0x40 0xFFFFFFFF  0x50 block table, 16 B per block: u32 start frame, u32 size, u64 0
  track records at align128(0x50 + 16*blocks) + 8, 48 B each (corrects the
  12*blocks + 8 of the first decode, which failed every file with 5+ blocks):
     f32[3] translation stride, f32[3] translation base, f32[3] rotation stride, f32[3] rotation base
  blocks at align128(end of records), each: u64 0, u32 tracks, u32 0, then per track
     (u64 0, u32 rotation keys, u32 0)(u64 0, u32 translation keys, u32 0),
     then per track rotation keys x 6 B (pad 4) and translation keys x 4 B; block padded to 128
  root-motion section: 16 B, f32 fps, f32[12] stride/base, two count entries, keys, 8 B pad
Frames = round(length * fps) + 1; consecutive blocks share a boundary frame.

Track names: the AMX that lists the animation carries the network's bone list;
its length equals the JAWB track count (checked here), so the AMX names the tracks.

Usage: python3 -I jawb_census.py <dir of JAWB files> <dir of AMX files>
"""
import collections
import math
import os
import struct
import sys
import importlib.util

here = os.path.dirname(os.path.abspath(__file__))


def load(name):
    spec = importlib.util.spec_from_file_location(name, os.path.join(here, name + '.py'))
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


ap = load('appearance_paths')
amx = load('amx_census')


def a128(x):
    return (x + 127) & -128


def walk(d):
    """Return (tracks, frames, end offset of blocks, root-motion end) or raise."""
    if d[:4] != b'JAWB' or struct.unpack_from('<I', d, 4)[0] != 2:
        raise ValueError('magic/version')
    length, fps = struct.unpack_from('<ff', d, 0x10)
    nb = struct.unpack_from('<I', d, 0x18)[0]
    nt = struct.unpack_from('<I', d, 0x30)[0]
    nf = round(length * fps) + 1
    starts = [struct.unpack_from('<I', d, 0x50 + 8 * i)[0] for i in range(nb)]
    rec0 = a128(0x50 + 16 * nb) + 8
    # sections start 8 bytes before a 128-byte boundary is reached: a block
    # follows at a128(end - 8); this only differs from a128(end) when the
    # end is 1..8 bytes past a boundary (track count a multiple of 8)
    pos = a128(rec0 + 48 * nt - 8)
    keys = 0
    for bi in range(nb):
        bf = (starts[bi + 1] - starts[bi] + 1) if bi + 1 < nb else nf - starts[bi]
        if struct.unpack_from('<I', d, pos + 8)[0] != nt:
            raise ValueError(f'block {bi} header')
        counts = [struct.unpack_from('<I', d, pos + 16 + 16 * k + 8)[0] for k in range(2 * nt)]
        if any(c > bf for c in counts):
            raise ValueError('key count > block frames')
        pos += 16 + 32 * nt
        for j in range(nt):
            rc, tc = counts[2 * j], counts[2 * j + 1]
            pos = (pos + 6 * rc + 3) & -4
            pos += 4 * tc
            keys += rc + tc
        pos = a128(pos)
    # root motion
    rfps = struct.unpack_from('<f', d, pos + 0x10)[0]
    if abs(rfps - fps) > 1e-3:
        raise ValueError(f'root fps {rfps} != {fps}')
    rc = struct.unpack_from('<I', d, pos + 0x48)[0]
    tc = struct.unpack_from('<I', d, pos + 0x58)[0]
    end = pos + 0x60
    end = (end + 6 * rc + 3) & -4
    end += 4 * tc
    return nt, nf, nb, keys, rc, tc, rec0, end


def first_frame_rotations(d, nt, rec0):
    """Track rotations at frame 0 as (x, y, z, w), w >= 0."""
    nb = struct.unpack_from('<I', d, 0x18)[0]
    recs = [struct.unpack_from('<12f', d, rec0 + 48 * i) for i in range(nt)]
    pos = a128(rec0 + 48 * nt)
    counts = [struct.unpack_from('<I', d, pos + 16 + 16 * k + 8)[0] for k in range(2 * nt)]
    pos += 16 + 32 * nt
    out = []
    for j in range(nt):
        rc, tc = counts[2 * j], counts[2 * j + 1]
        rs, rb = recs[j][6:9], recs[j][9:12]
        if rc:
            xr, yr, zr = struct.unpack_from('<HHH', d, pos)
            x, y, z = rb[0] + (xr & 0x7FFF) * rs[0], rb[1] + yr * rs[1], rb[2] + zr * rs[2]
            w = math.sqrt(max(0.0, 1 - x * x - y * y - z * z))
            out.append((x, y, z, w))
        else:
            out.append(None)
        pos = (pos + 6 * rc + 3) & -4
        pos += 4 * tc
    return out


def main():
    jdir, adir = sys.argv[1], sys.argv[2]
    c = collections.Counter()
    tracks = collections.Counter()
    meta = {}
    for fn in os.listdir(jdir):
        d = open(os.path.join(jdir, fn), 'rb').read()
        c['files'] += 1
        try:
            nt, nf, nb, keys, rc, tc, rec0, end = walk(d)
        except (ValueError, struct.error, IndexError) as e:
            c[f'x {e}' if 'fps' not in str(e) else 'x root fps'] += 1
            continue
        tail = len(d) - end
        if tail == 8:
            c['walked to the end (8 B pad)'] += 1
        elif tail > 16 and struct.unpack_from('<I', d, end + 8)[0] == nt:
            # bone-name table: u64, u32 bone count, u32, u64 index offset,
            # u64 name-offset offset, u64 names offset, u32 indices, names
            c['walked to the end + bone-name table'] += 1
        else:
            c[f'x {tail} unread bytes after root motion'] += 1
        tracks[nt] += 1
        c['keys'] += keys
        meta[int(fn[:16], 16)] = (nt, rec0)
    print(dict(c.most_common()))
    print('track counts', dict(tracks.most_common(12)))
    # join to AMX: does each AMX bone list length equal the track count of its animations?
    j = collections.Counter()
    pairs = collections.Counter()
    near = []
    for fn in os.listdir(adir):
        da = open(os.path.join(adir, fn), 'rb').read()
        ver, recs, skels, _, _ = amx.parse(da)
        for kind, name, folder, extra in recs:
            f = folder.replace('\\', '/')
            f = f if f.endswith('/') else f + '/'
            h = ap.hashlittle2(f'/resources/anim/{f}{name}.jba'.encode())
            if h not in meta:
                j['amx animation with no jba file'] += 1
                continue
            nt, rec0 = meta[h]
            if not skels:
                j['amx has no bone list'] += 1
                continue
            sk = skels[extra if extra is not None else 0]
            j['track count == bone count' if nt == len(sk) else 'x track count != bone count'] += 1
            if nt != len(sk):
                pairs[(nt, len(sk), len(skels))] += 1
            if extra is not None and nt == len(sk) and len(near) < 400:
                near.append((h, sk))
    print(dict(j))
    print('mismatch (tracks, bones, pose tables):', pairs.most_common(8))
    # is the indexed pose the animation's first frame?
    angles = []
    for h, sk in near:
        d = open(os.path.join(jdir, f'{h:016X}.bin'), 'rb').read()
        nt, nf, nb, keys, rc, tc, rec0, end = walk(d)
        ff = first_frame_rotations(d, nt, rec0)
        for k, q in enumerate(ff):
            if q is None:
                continue
            p = sk[k][1][:4]
            dot = abs(sum(a * b for a, b in zip(q, p)))
            n = math.sqrt(sum(a * a for a in p)) or 1
            angles.append(2 * math.degrees(math.acos(min(1.0, dot / n))))
    if angles:
        angles.sort()
        within = sum(a < 2 for a in angles)
        print(f'first-frame rotation vs AMX pose: {len(angles)} bones, median {angles[len(angles)//2]:.2f} deg, '
              f'{within} within 2 deg ({100*within/len(angles):.1f}%)')


if __name__ == '__main__':
    main()
