"""wem_reader -- read Wwise .wem audio (RIFF/WAVE, format tag 0xFFFF = Wwise
Vorbis) down to its packets, #429 criterion 7, audio.

Input: wem files (RIFF at byte 0, found by content), or bank DATA / acb
payloads carved by offset (wwise_banks / acb_reader). For each file:
  RIFF chunks: 'fmt ' (size 0x42 here: WAVEFORMATEX + vorb block inline at
  fmt+0x18), optional 'cue '/'LIST'/'smpl', 'data'.
  vorb (0x2A bytes, public layout of the ww2ogg / vgmstream readers):
    +0x00 sample count, +0x10 seek table size (the setup packet starts right
    after the seek table), +0x14 first audio packet offset (into data),
    +0x24 codebook hash, +0x28/+0x29 blocksize exponents.
  Packets: u16 size + payload (no granule); the setup packet must end exactly
  at the first audio packet, and audio packets must tile the data chunk.

Passing all checks proves the container and packet framing are fully read.
Decoding packets to PCM needs Wwise's codebook library, which the setup
packet refers to by index and which is not inside any .wem (see RESEARCH).

  python3 -I wem_reader.py <dir_of_wems> | python3 -I wem_reader.py --bnk <bank_dir> | --acb <acb_dir>
"""
import collections, os, struct, sys

mode = sys.argv[1]
st = collections.Counter(); fmt = collections.Counter(); chunks = collections.Counter(); bad = []
dur = 0.0


def wems():
    if mode == '--bnk':
        for fn in sorted(os.listdir(sys.argv[2])):
            b = open(os.path.join(sys.argv[2], fn), 'rb').read(); p = 0; didx = []; data = None
            while p + 8 <= len(b):
                tag = b[p:p + 4]; sz = struct.unpack_from('<I', b, p + 4)[0]
                if tag == b'DIDX':
                    didx = [struct.unpack_from('<III', b, p + 8 + i) for i in range(0, sz, 12)]
                if tag == b'DATA':
                    data = p + 8
                p += 8 + sz
            for wid, off, sz in didx:
                yield f'{fn}:{wid}', b[data + off:data + off + sz]
    elif mode == '--acb':
        for fn in sorted(os.listdir(sys.argv[2])):
            b = open(os.path.join(sys.argv[2], fn), 'rb').read()
            n = struct.unpack_from('<Q', b, 0)[0]; p = 8
            for _ in range(n):
                e = b.index(b'\0', p); name = b[p:e].decode('ascii', 'replace'); p = e + 1
                sz, off = struct.unpack_from('<QQ', b, p); p += 16
                yield f'{fn}:{name}', b[off:off + sz]
    else:
        for fn in sorted(os.listdir(mode)):
            yield fn, open(os.path.join(mode, fn), 'rb').read()


for name, b in wems():
    st['wems'] += 1
    try:
        assert b[:4] == b'RIFF' and b[8:12] == b'WAVE', 'not RIFF/WAVE'
        declared = struct.unpack_from('<I', b, 4)[0] + 8
        if declared > len(b):
            # A prefetch sound embeds only the head of its wem in the bank; the
            # whole file is the streamed /bnk2/streamed/<id>.wem.
            st['prefetch_head'] += 1
            if len(bad) < 3: bad.append((name, f'head {len(b)} of {declared}'))
            continue
        assert declared == len(b), 'RIFF size'
        p = 12; ch = {}
        while p + 8 <= len(b):
            tag = b[p:p + 4].decode('latin1'); sz = struct.unpack_from('<I', b, p + 4)[0]
            ch[tag] = (p + 8, sz); chunks[tag] += 1; p += 8 + sz + (sz & 1)
        fo, fs = ch['fmt ']
        tag, nch, rate = struct.unpack_from('<HHI', b, fo)
        fmt[(hex(tag), fs)] += 1
        if tag != 0xFFFF:
            st['non_vorbis'] += 1; continue
        assert fs == 0x42, f'fmt size {fs:#x}'
        v = fo + 0x18
        samples = struct.unpack_from('<I', b, v)[0]
        seek_size, audio_off = struct.unpack_from('<II', b, v + 0x10)
        setup_off = seek_size  # setup packet follows the seek table
        bs0, bs1 = b[v + 0x28], b[v + 0x29]
        assert 6 <= bs0 <= 13 and 6 <= bs1 <= 13 and bs0 <= bs1, 'blocksizes'
        do, ds = ch['data']
        q = do + setup_off
        sp = struct.unpack_from('<H', b, q)[0]
        assert setup_off + 2 + sp == audio_off, 'setup packet ends at first audio packet'
        q = do + audio_off; n = 0
        while q < do + ds:
            ps = struct.unpack_from('<H', b, q)[0]; q += 2 + ps; n += 1
        assert q == do + ds, f'packets overrun data by {q - do - ds}'
        st['framing_ok'] += 1; st['audio_packets'] += n
        dur += samples / rate
    except (AssertionError, KeyError, struct.error, IndexError) as e:
        st['bad'] += 1
        if len(bad) < 10: bad.append((name, str(e)))
print(dict(st)); print('fmt', dict(fmt)); print('chunks', dict(chunks)); print('hours', round(dur / 3600, 1)); print('bad', bad)
