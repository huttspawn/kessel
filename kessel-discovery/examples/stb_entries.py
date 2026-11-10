"""Dump STB entries: id1 id2 flags version textlen text."""
import struct, sys


def entries(path):
    d = open(path, 'rb').read()
    n = struct.unpack_from('<I', d, 3)[0]
    for i in range(n):
        o = 7 + 26 * i
        id1, id2, fl, ver, tl, to, al = struct.unpack_from('<IIHIIII', d, o)
        t = d[to:to + tl].decode('utf-8', 'replace').rstrip('\0') if tl else ''
        yield id1, id2, fl, ver, tl, al, t


if __name__ == '__main__':
    for e in entries(sys.argv[1]):
        print('%d\t%d\t%04x\t%d\t%d\t%d\t%s' % (e[0], e[1], e[2], e[3], e[4], e[5], e[6][:70]))
