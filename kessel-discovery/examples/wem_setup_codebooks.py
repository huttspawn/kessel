"""wem_setup_codebooks -- read the Wwise Vorbis setup packet of every .wem far
enough to see how it names its codebooks (#429 criterion 7, audio).

The setup packet (located by wem_reader: data + seek table size, u16 size
then payload) has no '\\x05vorbis' header. Its first field is the codebook
count minus one (8 bits); in the external-codebook form each codebook is then a
10-bit index into a codebook library the decoder holds. Bits are read
LSB-first, as in Vorbis. If every index in every file falls inside one small
range, the codebooks are external and the files cannot be decoded to PCM from
their own bytes.

Inputs as wem_reader: a directory of wems, --bnk <bank_dir> (DIDX/DATA), or
--acb <acb_dir> (VO containers: u64 count, then per record name\\0, u64 size,
u64 offset).

  python3 -I wem_setup_codebooks.py <dir> | --bnk <dir> | --acb <dir>
"""
import collections, os, struct, sys


def wems(mode, d):
    for fn in sorted(os.listdir(d)):
        b = open(os.path.join(d, fn), 'rb').read()
        if mode == '--bnk':
            p = 0; didx = []; data = None
            while p + 8 <= len(b):
                tag = b[p:p + 4]; sz = struct.unpack_from('<I', b, p + 4)[0]
                if tag == b'DIDX':
                    didx = [struct.unpack_from('<III', b, p + 8 + i) for i in range(0, sz, 12)]
                if tag == b'DATA':
                    data = p + 8
                p += 8 + sz
            for wid, off, sz in didx:
                yield b[data + off:data + off + sz]
        elif mode == '--acb':
            if len(b) < 16:
                continue
            n = struct.unpack_from('<Q', b, 0)[0]; p = 8
            for _ in range(n):
                e = b.index(b'\0', p); p = e + 1
                sz, off = struct.unpack_from('<QQ', b, p); p += 16
                yield b[off:off + sz]
        else:
            yield b


class Bits:
    def __init__(self, b):
        self.b = b; self.p = 0

    def read(self, n):
        v = 0
        for i in range(n):
            byte = self.b[self.p >> 3]
            v |= ((byte >> (self.p & 7)) & 1) << i
            self.p += 1
        return v


mode = sys.argv[1] if sys.argv[1].startswith('--') else 'dir'
d = sys.argv[-1]
st = collections.Counter(); ids = collections.Counter(); counts = collections.Counter()
for w in wems(mode, d):
    if w[:4] != b'RIFF':
        st['not_riff'] += 1; continue
    ch = {}; q = 12
    while q + 8 <= len(w):
        t = w[q:q + 4]; s = struct.unpack_from('<I', w, q + 4)[0]
        ch[t] = (q + 8, s); q += 8 + s + (s & 1)
    if b'fmt ' not in ch or b'data' not in ch:
        st['no_chunks'] += 1; continue
    fo, fs = ch[b'fmt ']
    if struct.unpack_from('<H', w, fo)[0] != 0xFFFF or fs != 0x42:
        st['not_wwise_vorbis'] += 1; continue
    v = fo + 0x18
    seek = struct.unpack_from('<I', w, v + 0x10)[0]
    do = ch[b'data'][0]
    sp = struct.unpack_from('<H', w, do + seek)[0]
    pk = w[do + seek + 2:do + seek + 2 + sp]
    if len(pk) < sp:
        st['setup_truncated'] += 1; continue
    r = Bits(pk)
    try:
        n = r.read(8) + 1
        counts[n] += 1
        for _ in range(n):
            ids[r.read(10)] += 1
        st['ok'] += 1
    except IndexError:
        st['short'] += 1
print(dict(st))
print('codebook count per setup: min', min(counts) if counts else None, 'max', max(counts) if counts else None)
print('distinct codebook indices', len(ids), 'min', min(ids) if ids else None, 'max', max(ids) if ids else None)
