"""Read room .dat instances (ROOM_DAT_BINARY_FORMAT_): id, asset id and typed props. usage: world_room.py <room.dat> [ids]"""
import struct, sys

MAGIC = bytes.fromhex('3412cdab')


def props(b, o, n):
    out = []
    for _ in range(n):
        t = b[o]; k = struct.unpack_from('<I', b, o + 1)[0]; o += 5
        if t == 0:
            v = b[o]; o += 1
        elif t in (1, 3):
            v = struct.unpack_from('<I', b, o)[0]; o += 4
        elif t == 4:
            v = struct.unpack_from('<f', b, o)[0]; o += 4
        elif t == 5:
            v = struct.unpack_from('<Q', b, o)[0]; o += 8
        elif t == 6:
            v = struct.unpack_from('<3f', b, o); o += 12
        elif t == 8:
            ln = struct.unpack_from('<I', b, o)[0]; o += 4
            v = b[o:o + ln].decode('utf-16-le', 'replace'); o += ln
        elif t == 9:
            ln = struct.unpack_from('<I', b, o)[0]; o += 4
            v = b[o:o + ln].hex()[:80]; o += ln
        else:
            out.append(('?', t, hex(k), b[o:o+16].hex()))
            return out, o, False
        out.append((t, '%08x' % k, v))
    return out, o, True


def instances(b):
    i = 0
    while True:
        i = b.find(MAGIC, i)
        if i < 0:
            return
        o = i + 4
        if b[o] != 0:
            i += 1; continue
        iid, aid = struct.unpack_from('<QQ', b, o + 1)
        o += 17
        z0 = b[o]; cnt, nbytes = struct.unpack_from('<II', b, o + 1); z1 = b[o + 9]
        o += 10
        ps, end, ok = props(b, o, cnt)
        yield i, iid, aid, (z0, cnt, nbytes, z1), ps, ok, end - o == nbytes
        i += 4


if __name__ == '__main__':
    b = open(sys.argv[1], 'rb').read()
    want = set(int(x, 16) for x in sys.argv[2:])
    for inst in instances(b):
        if not want or inst[2] in want or inst[1] in want:
            print(inst)
