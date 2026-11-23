"""fxe_census -- decode push 3 (#429): frame every FaceFX file (.fxe/.fxa,
magic 'FACE', OC3 FaceFX archive written by the 'Bioware Edmonton' licence)
to its last byte.

Layout tried here (little endian; str32 = u32 length + bytes):
  'FACE' u32 version  u32 0  str32 licensee  str32 project  u32  u16
  u32 class count, per class: u32 id, u32 1, str32 class name, u16 class version
  u32 name count, per name: str32 (the archive's name table; later fields are
  indices into it)
  u32 0  u32 0  u32 group count, per group:
    u32 group name index  u32 animation count, per animation:
      u32 animation name index  u32 curve count
      per curve: u32 curve name index (a phoneme target such as 'Oat',
                 a gesture 'E_GESTURE_...', 'Blink', 'Gaze Eye Yaw'), u32 flags
      u32 key count, per key: f32 time, f32 value, f32 slope in, f32 slope out
      u32 curve count again, per curve: u32 keys owned (in curve order; sums
      to the key count)
      f32 start time  f32 end time
      then a tail read by tail() below
Run: python3 -I fxe_census.py <dir of FACE files>
"""
import collections
import os
import struct
import sys


class Bad(Exception):
    pass


def parse(d, c):
    if d[:4] != b'FACE':
        raise Bad('magic')
    o = 12

    def u32():
        nonlocal o
        v = struct.unpack_from('<I', d, o)[0]
        o += 4
        return v

    def s32():
        nonlocal o
        n = u32()
        if n > 4096:
            raise Bad('string length')
        s = d[o:o + n]
        o += n
        return s

    s32()
    s32()
    u32()
    o += 2
    for _ in range(u32()):
        u32()
        u32()
        s32()
        o += 2
    names = [s32() for _ in range(u32())]
    pre = (u32(), u32())
    c[f'pre {pre}'] += 1
    groups = u32()
    anims = 0
    for _ in range(groups):
        if u32() >= len(names):
            raise Bad('group name index')
        for _ in range(u32()):
            anims += 1
            if u32() >= len(names):
                raise Bad('anim name index')
            nc = u32()
            for _ in range(nc):
                if u32() >= len(names):
                    raise Bad('curve name index')
                c[f'curve flags {u32()}'] += 1
            nk = u32()
            o += 16 * nk
            if u32() != nc:
                raise Bad('second curve count')
            per = struct.unpack_from(f'<{nc}I', d, o)
            o += 4 * nc
            if sum(per) != nk:
                raise Bad('key counts do not sum')
            o += 8  # start, end
            o = tail(d, o, c, names)
            c['keys'] += nk
            c['curves'] += nc
    c['animations'] += anims
    return o


def tail(d, o, c, names):
    """Per-animation tail. Recorded as raw u32s so the census shows its shape."""
    w = struct.unpack_from('<4i', d, o)
    c[f'tail {w}'] += 1
    return o + 16


def main():
    src = sys.argv[1]
    c = collections.Counter()
    for fn in os.listdir(src):
        d = open(os.path.join(src, fn), 'rb').read()
        c['files'] += 1
        try:
            end = parse(d, c)
        except (Bad, struct.error) as e:
            c[f'x {e}'] += 1
            continue
        c['parsed to the last byte' if end == len(d) else f'x {len(d) - end} bytes left'] += 1
    for k, v in sorted(c.items(), key=lambda x: -x[1])[:40]:
        print(f'{k:<60} {v:>9}')


if __name__ == '__main__':
    main()
