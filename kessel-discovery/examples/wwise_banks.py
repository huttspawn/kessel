"""wwise_banks -- read every Wwise SoundBank (.bnk, magic BKHD) in the install
and report its sections, HIRC object types, sounds and their sources, events
and their actions (#429 criterion 7, audio).

Input: a directory of bank files found by content (BKHD at byte 0; media_census
+ extract_by_hash, no dictionary). Bank version 62 layout (Wwise 2012-era):

  section  = tag[4] u32 size body
  BKHD     = u32 version, u32 bank id, ...
  DIDX     = n x (u32 wem id, u32 offset into DATA, u32 size)
  DATA     = concatenated embedded wems (RIFF)
  HIRC     = u32 count, n x (u8 type, u32 size, u32 object id, body)
  STID     = u32 type, u32 count, n x (u32 bank id, u8 len, name)

Output JSON: per-bank summary plus global tables (event id -> action ids,
action id -> type/target, sound id -> source wem/stream type) used by
wwise_names.py to join game-side names onto banks.

  python3 -I wwise_banks.py <bank_dir> <out.json>
"""
import collections, json, os, struct, sys

src, out = sys.argv[1], sys.argv[2]
HTYPE = {1: 'Settings', 2: 'Sound', 3: 'Action', 4: 'Event', 5: 'RanSeqCntr', 6: 'SwitchCntr',
         7: 'ActorMixer', 8: 'Bus', 9: 'LayerCntr', 10: 'MusicSegment', 11: 'MusicTrack',
         12: 'MusicSwitch', 13: 'MusicRanSeq', 14: 'Attenuation', 15: 'DialogueEvent',
         16: 'FeedbackBus', 17: 'FeedbackNode', 18: 'FxShareSet', 19: 'FxCustom', 20: 'AuxBus'}
STREAM = {0: 'embedded', 1: 'streamed', 2: 'prefetch'}

banks = {}; versions = collections.Counter(); sec_count = collections.Counter()
htypes = collections.Counter(); stream = collections.Counter(); codecs = collections.Counter()
events = {}; actions = {}; sounds = {}; bank_names = {}; errors = []
didx_total = 0; didx_riff_ok = 0
music_sources = {}
for fn in sorted(os.listdir(src)):
    b = open(os.path.join(src, fn), 'rb').read()
    p = 0; info = {'sections': [], 'hirc': collections.Counter()}
    data_off = None; didx = []
    try:
        while p + 8 <= len(b):
            tag = b[p:p + 4].decode('latin1'); size = struct.unpack_from('<I', b, p + 4)[0]
            body = b[p + 8:p + 8 + size]
            info['sections'].append(tag); sec_count[tag] += 1
            if tag == 'BKHD':
                ver, bid = struct.unpack_from('<II', body, 0)
                info['version'] = ver; info['bank_id'] = bid; versions[ver] += 1
            elif tag == 'DIDX':
                for i in range(0, size, 12):
                    didx.append(struct.unpack_from('<III', body, i))
            elif tag == 'DATA':
                data_off = p + 8
            elif tag == 'STID':
                _, n = struct.unpack_from('<II', body, 0); q = 8
                for _ in range(n):
                    bid, ln = struct.unpack_from('<IB', body, q); q += 5
                    bank_names[bid] = body[q:q + ln].decode('latin1'); q += ln
            elif tag == 'HIRC':
                n = struct.unpack_from('<I', body, 0)[0]; q = 4
                for _ in range(n):
                    t, sz, oid = struct.unpack_from('<BII', body, q)
                    ob = body[q + 9:q + 5 + sz]
                    tn = HTYPE.get(t, f't{t}')
                    info['hirc'][tn] += 1; htypes[tn] += 1
                    if t == 2:  # Sound: plugin id, stream type, source id, file id
                        plug, st, sid, fid = struct.unpack_from('<IIII', ob, 0)
                        sounds[oid] = (sid, STREAM.get(st, st), plug, info.get('bank_id'))
                        stream[STREAM.get(st, st)] += 1; codecs[plug] += 1
                    elif t == 11:  # MusicTrack: u32 nsrc, then sources like Sound
                        # AkBankSourceData: plugin u32, stream type u32, source id
                        # u32, file id u32, [offset u32, size u32 if embedded],
                        # flags u8 -- variable size, so walk it.
                        nsrc = struct.unpack_from('<I', ob, 0)[0]; r = 4
                        for k in range(nsrc):
                            plug, st, sid, fid = struct.unpack_from('<IIII', ob, r)
                            r += 16 + (8 if st == 0 else 0) + 1
                            music_sources[sid] = STREAM.get(st, st)
                    elif t == 4:  # Event: u32 count, action ids
                        cnt = struct.unpack_from('<I', ob, 0)[0]
                        events[oid] = (list(struct.unpack_from(f'<{cnt}I', ob, 4)), info.get('bank_id'))
                    elif t == 3:  # Action: u16 type, u32 target id
                        at, tgt = struct.unpack_from('<HI', ob, 0)
                        actions[oid] = (at, tgt)
                    q += 5 + sz
            p += 8 + size
        if data_off is not None:
            for wid, off, sz in didx:
                didx_total += 1
                didx_riff_ok += b[data_off + off:data_off + off + 4] == b'RIFF'
        info['didx'] = len(didx)
        info['didx_ids'] = [w for w, _, _ in didx]
    except Exception as e:  # noqa: BLE001 -- census: record and continue
        errors.append((fn, repr(e)))
    info['hirc'] = dict(info['hirc']); banks[fn] = info

at_count = collections.Counter(a[0] for a in actions.values())
R = {'banks': len(banks), 'versions': dict(versions), 'sections': dict(sec_count),
     'hirc_types': dict(htypes), 'sound_stream_types': dict(stream),
     'sound_codecs': {hex(k): v for k, v in codecs.items()},
     'didx_wems': didx_total, 'didx_riff_ok': didx_riff_ok,
     'events': len(events), 'actions': len(actions), 'action_types': {hex(k): v for k, v in at_count.items()},
     'bank_names_in_stid': len(bank_names), 'errors': errors,
     'music_track_sources': len(music_sources)}
json.dump({'summary': R, 'bank_names': bank_names, 'per_bank': banks,
           'events': events, 'actions': actions, 'sounds': sounds,
           'music_sources': music_sources}, open(out, 'w'))
print(json.dumps(R, indent=1))
