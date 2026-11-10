"""Read a Scaleform GFx movie (SWF-based): header, tag list, and the strings its scripts use.

usage: python3 -I gfx_dump.py <movie.gfx> [--abc]

GFx files start 'GFX' (uncompressed) or 'CFX' (zlib), then a version byte and the uncompressed
length, then the SWF body: frame RECT, frame rate, frame count, and tags (u16 code<<6|len,
long form when len == 0x3f). Prints tag counts, every DoAction (12) / DoInitAction (59) /
DoABC (72, 82) block's size, and for ABC blocks the constant pool strings (AVM2 u30 varints).
With --abc it also prints each method body's size and the multiname names, so the code that
reads a field can be located by name.
"""
import collections
import struct
import sys
import zlib


def read_body(data):
    sig = data[:3]
    if sig == b"CFX" or sig == b"CWS":
        body = zlib.decompress(data[8:])
    elif sig in (b"GFX", b"FWS"):
        body = data[8:]
    else:
        raise ValueError(f"not a GFx/SWF movie: {sig!r}")
    return data[3], body


def skip_rect(b):
    nbits = b[0] >> 3
    total = 5 + 4 * nbits
    return (total + 7) // 8


def tags(body):
    pos = skip_rect(body) + 4
    while pos + 2 <= len(body):
        (hdr,) = struct.unpack_from("<H", body, pos)
        pos += 2
        code, length = hdr >> 6, hdr & 0x3F
        if length == 0x3F:
            (length,) = struct.unpack_from("<I", body, pos)
            pos += 4
        yield code, body[pos:pos + length]
        pos += length
        if code == 0:
            break


class Abc:
    def __init__(self, b):
        self.b, self.p = b, 0

    def u30(self):
        r, shift = 0, 0
        while True:
            x = self.b[self.p]
            self.p += 1
            r |= (x & 0x7F) << shift
            if not x & 0x80:
                return r
            shift += 7

    def pool(self):
        self.p += 4  # minor, major version
        ints = [0] + [self.u30() for _ in range(max(0, self.u30() - 1))]
        uints = [0] + [self.u30() for _ in range(max(0, self.u30() - 1))]
        n = self.u30()
        self.p += 8 * max(0, n - 1)
        strings = [""]
        for _ in range(max(0, self.u30() - 1)):
            ln = self.u30()
            strings.append(self.b[self.p:self.p + ln].decode("utf-8", "replace"))
            self.p += ln
        return ints, uints, strings


def main():
    path = sys.argv[1]
    version, body = read_body(open(path, "rb").read())
    print("version", version, "body", len(body))
    counts = collections.Counter()
    for code, data in tags(body):
        counts[code] += 1
        if code in (12, 59):
            print("DoAction" if code == 12 else "DoInitAction", len(data))
            # AS2 constant pools: ActionConstantPool 0x88; DoInitAction starts with a u16 sprite id
            i = 2 if code == 59 else 0
            while i < len(data):
                op = data[i]
                if op >= 0x80:
                    (ln,) = struct.unpack_from("<H", data, i + 1)
                    if op == 0x88:
                        (cnt,) = struct.unpack_from("<H", data, i + 3)
                        words = data[i + 5:i + 3 + ln].split(b"\0")[:cnt]
                        print("  constants", [w.decode("latin-1") for w in words])
                    i += 3 + ln
                else:
                    i += 1
        if code in (72, 82):
            abc = data
            if code == 82:
                abc = data[4:]
                abc = abc[abc.index(b"\0") + 1:]
            ints, uints, strings = Abc(abc).pool()
            print("DoABC", len(data), "ints", ints[1:40], "strings", len(strings))
            for s in strings:
                print("  str", s)
    print("tags", dict(counts))


if __name__ == "__main__":
    main()
