"""anim_rig -- decode push 3 (#429): the morpheme network (.mph, magic 'MAWB')
holds the rig that names the anonymous JAWB animation tracks.

MAWB header read here (little endian), on every file:
  'MAWB' u32 2  u32 2 ...  0x58 u32 rig bone count
  0x68 i32[count] parent index per bone (-1 for bone 0, parent < child)
  bone names: null-terminated, in bone order, starting with
  'CharacterWorldSpaceTM' (the trajectory bone)

The joins tested:
  x.mph.amx (the AMX animation list) and x.mph (the network) are siblings by
  path; the AMX lists each animation's .jba; every .jba of a network has
  rig count - 1 tracks: track i is rig bone i + 1, and the JAWB root-motion
  section is bone 0 (CharacterWorldSpaceTM). The .jba files that carry their
  own bone-name table are compared name for name with the rig.

The dictionary is used only to pair an .amx with its .mph by path.

Usage: python3 -I anim_rig.py <MAWB dir> <AMX dir> <JAWB dir> <hashes_filename.txt>
"""
import collections
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
jawb = load('jawb_census')

ROOT = b'CharacterWorldSpaceTM\0'


def rig(d):
    """Return (names, parents) or raise ValueError."""
    if d[:4] != b'MAWB':
        raise ValueError('magic')
    n = struct.unpack_from('<I', d, 0x58)[0]
    if n == 0:
        return [], []
    parents = list(struct.unpack_from(f'<{n}i', d, 0x68))
    if parents[0] != -1 or any(not (0 <= p < i) for i, p in enumerate(parents) if i):
        raise ValueError('parents')
    o = d.find(ROOT)
    if o < 0:
        raise ValueError('no root name')
    names = []
    for _ in range(n):
        e = d.index(b'\0', o)
        names.append(d[o:e].decode('latin-1'))
        o = e + 1
    return names, parents


def jawb_names(d, end, nt):
    """The optional bone-name table after the root-motion section."""
    if len(d) - end <= 16:
        return None
    b = end + 8
    n, _, oi, on, onm = struct.unpack_from('<IIQQQ', d, b)
    if n != nt:
        return None
    offs = struct.unpack_from(f'<{n}I', d, b + on)
    out = []
    for o in offs:
        s = d[b + onm + o:]
        out.append(s[:s.index(b'\0')].decode('latin-1'))
    return out


def main():
    mdir, adir, jdir, dictp = sys.argv[1:5]
    c = collections.Counter()
    rigs = {}
    for fn in os.listdir(mdir):
        d = open(os.path.join(mdir, fn), 'rb').read()
        c['mph files'] += 1
        try:
            names, parents = rig(d)
        except (ValueError, struct.error) as e:
            c[f'x mph {e}'] += 1
            continue
        c['mph rig read (parents before children, names from CharacterWorldSpaceTM)' if names else 'mph with no rig (count 0)'] += 1
        rigs[int(fn[:16], 16)] = names
    # amx hash -> mph hash, via the dictionary path
    amx_to_mph = {}
    for line in open(dictp, errors='replace'):
        p = line.split('#')
        if len(p) >= 3 and p[2].endswith('.mph.amx'):
            h = (int(p[0], 16) << 32) | int(p[1], 16)
            amx_to_mph[h] = ap.hashlittle2(p[2][:-4].encode())
    jmeta = {}
    for fn in os.listdir(jdir):
        d = open(os.path.join(jdir, fn), 'rb').read()
        try:
            nt, nf, nb, keys, rc, tc, rec0, end = jawb.walk(d)
        except (ValueError, struct.error, IndexError):
            continue
        jmeta[int(fn[:16], 16)] = (nt, jawb_names(d, end, nt))
    for fn in os.listdir(adir):
        h = int(fn[:16], 16)
        if h not in amx_to_mph:
            c['amx with no dictionary path'] += 1
            continue
        mh = amx_to_mph[h]
        if mh not in rigs:
            c['amx whose .mph sibling is absent or unread'] += 1
            continue
        names = rigs[mh]
        c['amx paired with its .mph'] += 1
        ver, recs, skels, _, _ = amx.parse(open(os.path.join(adir, fn), 'rb').read())
        for kind, name, folder, extra in recs:
            f = folder.replace('\\', '/')
            f = f if f.endswith('/') else f + '/'
            jh = ap.hashlittle2(f'/resources/anim/{f}{name}.jba'.encode())
            if jh not in jmeta:
                c['animation with no .jba'] += 1
                continue
            nt, jn = jmeta[jh]
            c['jba tracks == rig bones - 1' if nt == len(names) - 1 else 'x jba tracks != rig bones - 1'] += 1
            if nt != len(names) - 1:
                c[f'x mismatch tracks {nt} rig {len(names)}'] += 1
            if jn is not None:
                c['jba names == rig names[1:]' if jn == names[1:] else 'x jba names != rig names[1:]'] += 1
            if skels and extra is not None:
                sk = [b[0] for b in skels[extra]]
                c['amx pose bones == rig names[1:]' if sk == names[1:] else 'amx pose bones != rig names[1:]'] += 1
    for k, v in sorted(c.items()):
        if v and not (k.startswith('x mismatch') and v < 50):
            print(f'{k:<75} {v:>9}')


if __name__ == '__main__':
    main()
