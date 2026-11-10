"""acb_reader -- read SWTOR VO containers (.acb, /resources/<locale>/bnk2/),
#429 criterion 7, audio.

Layout (proved here over every file given):
  u64 count
  count x { name: NUL-terminated ASCII '<cnv fqn _>_<node>[_variant]_<m|f>.wem',
            u64 size, u64 offset }
  wem payloads (RIFF/WAVE, Wwise Vorbis) at the offsets, back to back.

Checks: every offset lands on 'RIFF'; RIFF chunk size + 8 == record size;
records tile the file from the end of the table to EOF with no gap or overlap.
Content rule used to find the files (no dictionary): u32 count > 0, u32 0,
then 'cnv_'.

  python3 -I acb_reader.py <acb_dir> [out.tsv]
"""
import collections, os, re, struct, sys

d = sys.argv[1]; out = open(sys.argv[2], 'w') if len(sys.argv) > 2 else None
st = collections.Counter(); fmt = collections.Counter(); names = collections.Counter()
bad = []
for fn in sorted(os.listdir(d)):
    b = open(os.path.join(d, fn), 'rb').read()
    n = struct.unpack_from('<Q', b, 0)[0]; p = 8; recs = []
    try:
        for _ in range(n):
            e = b.index(b'\0', p); name = b[p:e].decode('ascii'); p = e + 1
            size, off = struct.unpack_from('<QQ', b, p); p += 16
            recs.append((name, size, off))
    except (ValueError, struct.error, UnicodeDecodeError) as x:
        bad.append((fn, 'table', repr(x))); continue
    st['files'] += 1; st['records'] += n
    ok = True; cur = p
    for name, size, off in sorted(recs, key=lambda r: r[2]):
        riff = b[off:off + 4] == b'RIFF'
        rsz = struct.unpack_from('<I', b, off + 4)[0] + 8 if riff else -1
        st['riff_at_offset'] += riff; st['riff_size_matches'] += rsz == size
        st['contiguous'] += off == cur; cur = off + size
        if riff:
            tag, ch, rate = struct.unpack_from('<HHI', b, off + 20)
            fmt[(hex(tag), ch, rate)] += 1
        m = re.fullmatch(r'(.+?)_(\d+)((?:_[a-z0-9_]+?)?)_([mf])\.wem', name)
        names['node-numbered' if m else 'other'] += 1
        if out: out.write(f'{fn}\t{name}\t{size}\t{off}\n')
        ok &= riff and rsz == size
    st['tiles_to_eof'] += cur == len(b)
    st['files_all_ok'] += ok and cur == len(b)
print(dict(st)); print('wem fmt', dict(fmt)); print('names', dict(names)); print('bad', bad[:5], len(bad))
