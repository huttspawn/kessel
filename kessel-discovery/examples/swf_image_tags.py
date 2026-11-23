"""swf_image_tags -- the non-DDS image containers in Assets: SWF/GFx movies
(CWS zlib, 'GFX' Scaleform) and stray PNGs (#429 criterion 7, textures).

For each file: header magic, version, decompressed length check, then every
tag (RECORDHEADER: u16 code<<6|len, len 0x3f -> u32), counting image tags
(DefineBits 6, JPEGTables 8, DefineBitsJPEG2 21, DefineBitsLossless 20,
DefineBitsJPEG3 35, DefineBitsLossless2 36, DefineBitsJPEG4 90) and the
Scaleform extension tags (code >= 1000, e.g. DefineExternalImage 1001,
DefineExternalImage2 1009, DefineSubImage 1008). PNG: IHDR size and IEND.

  python3 -I swf_image_tags.py <dir>
"""
import collections, os, struct, sys, zlib

IMG = {6: 'DefineBits', 8: 'JPEGTables', 20: 'DefineBitsLossless', 21: 'DefineBitsJPEG2',
       35: 'DefineBitsJPEG3', 36: 'DefineBitsLossless2', 90: 'DefineBitsJPEG4',
       1001: 'GFx:DefineExternalImage', 1008: 'GFx:DefineSubImage', 1009: 'GFx:DefineExternalImage2'}
d = sys.argv[1]
for fn in sorted(os.listdir(d)):
    b = open(os.path.join(d, fn), 'rb').read()
    if b[:8] == b'\x89PNG\r\n\x1a\n':
        w, h = struct.unpack_from('>II', b, 16)
        print(fn, 'PNG', f'{w}x{h}', 'IEND' if b[-8:-4] == b'IEND' else 'no IEND', len(b)); continue
    sig = b[:3]
    ver = b[3]
    ln = struct.unpack_from('<I', b, 4)[0]
    if sig in (b'CWS', b'CFX'):
        body = zlib.decompress(b[8:])
    elif sig in (b'FWS', b'GFX'):
        body = b[8:]
    else:
        print(fn, 'unknown', b[:8].hex()); continue
    ok = len(body) + 8 == ln
    nbits = body[0] >> 3
    p = (5 + 4 * nbits + 7) // 8 + 4  # RECT + frame rate + frame count
    tags = collections.Counter(); n = 0
    while p + 2 <= len(body):
        cl = struct.unpack_from('<H', body, p)[0]; p += 2
        code, l = cl >> 6, cl & 0x3f
        if l == 0x3f:
            l = struct.unpack_from('<I', body, p)[0]; p += 4
        tags[IMG.get(code, 'other' if code < 1000 else f'gfx{code}')] += 1
        n += 1; p += l
        if code == 0:
            break
    print(fn, sig.decode(), 'v', ver, 'len_ok', ok, 'tags', n, 'ends_at', p == len(body), dict(tags))
