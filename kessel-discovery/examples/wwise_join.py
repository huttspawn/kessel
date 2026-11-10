"""wwise_join -- how game data names its sounds, and whether every link from a
name to playable audio closes in the client (#429 criterion 7, audio).

No dictionary: banks, wems, appearance specs (.epp), effect specs (.fxspec)
and area files are found by content (media_census); paths are derived and
hashed the archive's way (hash_paths).

 A. Bank names: STID tables map bank id -> name; /resources/bnk2/<name>.bnk
    is hashed and looked up among BKHD entries.
 B. Streamed audio: every streamed/prefetch Sound and MusicTrack source id ->
    /resources/bnk2/streamed/<id>.wem, looked up among RIFF entries.
 C. Embedded audio: every embedded Sound source id is looked up in the union
    of all banks' DIDX tables (whose wems sit in DATA as RIFF).
 D. Event names: Wwise addresses events by FNV-1 32 of the lowercased name.
    Names are read from epp PlaySound (sndPlayEvt, sndStopEvt), PlayGrunt
    (sndGruntEvent), fxspec sound lists (fxspec_links), and every identifier
    string in area/room .dat files; each is hashed and looked up among HIRC
    Event ids.
 E. Event -> Action -> target: each action target id is looked up among all
    HIRC object ids.
 F. Streamed .wem RIFF header census (format tag, channels, rate).

  python3 -I wwise_join.py <wwise.json> <census.tsv> <epp_dir> <dat_dir> <hash_paths> <out.json>
"""
import collections, json, os, re, struct, subprocess, sys
import xml.etree.ElementTree as ET

wj, cen, eppd, datd, hp, out = sys.argv[1:7]
W = json.load(open(wj))


def fnv1_32(s):
    h = 0x811C9DC5
    for c in s.lower().encode():
        h = (h * 0x01000193) & 0xFFFFFFFF
        h ^= c
    return h


def hashp(paths):
    r = subprocess.run([hp], input='\n'.join(paths), capture_output=True, text=True).stdout
    return {p: h for h, p in (l.split('\t', 1) for l in r.splitlines())}


census = {}; riff_heads = {}
for l in open(cen, newline=''):
    a, h, comp, cs, us, cls, head = l.rstrip('\n').split('\t')
    census[h] = cls
    if cls == 'RIFF':
        riff_heads[h] = bytes.fromhex(head)
R = {}

# A. bank names
names = W['bank_names']
hp_b = hashp([f'/resources/bnk2/{n}.bnk' for n in names.values()])
bkhd = {h for h, c in census.items() if c == 'BKHD'}
named = {hp_b[f'/resources/bnk2/{n}.bnk'] for n in names.values()} & bkhd
own_ids = {v.get('bank_id') for v in W['per_bank'].values()}
R['A_banks'] = {'stid_names': len(names), 'bkhd_entries': len(bkhd),
                'bkhd_entries_named_by_stid': len(named),
                'bank_ids_with_stid_name': len(own_ids & {int(k) for k in names})}

# B. streamed
sounds = W['sounds']
streamed = {s[0] for s in sounds.values() if s[1] in ('streamed', 'prefetch')}
streamed |= {int(k) for k, v in W['music_sources'].items() if v in ('streamed', 'prefetch')}
hp_s = hashp([f'/resources/bnk2/streamed/{i}.wem' for i in sorted(streamed)])
riff = set(riff_heads)
reached = {hp_s[f'/resources/bnk2/streamed/{i}.wem'] for i in streamed}
R['B_streamed'] = {'streamed_source_ids': len(streamed), 'found_as_RIFF': len(reached & riff),
                   'riff_entries': len(riff), 'riff_not_reached': len(riff - reached)}

# C. embedded
didx = set()
for v in W['per_bank'].values():
    didx.update(v.get('didx_ids', []))
emb = {s[0] for s in sounds.values() if s[1] == 'embedded'}
emb_music = {int(k) for k, v in W['music_sources'].items() if v == 'embedded'}
R['C_embedded'] = {'embedded_sound_sources': len(emb), 'in_some_DIDX': len(emb & didx),
                   'embedded_music_sources': len(emb_music), 'music_in_DIDX': len(emb_music & didx),
                   'didx_ids': len(didx), 'didx_never_referenced': len(didx - emb - emb_music)}

# D. event names
ev = {int(k) for k in W['events']}
src_names = collections.defaultdict(collections.Counter)
for fn in os.listdir(eppd):
    b = open(os.path.join(eppd, fn), 'rb').read()
    t = (b.decode('utf-16-le', 'replace') if (b[:2] == b'\xff\xfe' or b[1:2] == b'\0') else b.decode('utf-8', 'replace')).lstrip('﻿')
    try:
        root = ET.fromstring(t)
    except ET.ParseError:
        continue
    for a in root.iter('AppearanceAction'):
        for tag in ('sndPlayEvt', 'sndStopEvt', 'sndGruntEvent'):
            x = a.find(tag)
            if x is not None and (x.text or '').strip():
                src_names[f'epp.{tag}'][x.text.strip()] += 1
dat_words = collections.Counter()
for fn in os.listdir(datd):
    b = open(os.path.join(datd, fn), 'rb').read()
    for m in re.finditer(rb'[A-Za-z][A-Za-z0-9_]{5,}', b):
        dat_words[m.group().decode()] += 1
    for m in re.finditer(rb'(?:[A-Za-z_0-9]\x00){6,}', b):
        dat_words[m.group().decode('utf-16-le')] += 1
for w, n in dat_words.items():
    if w.lower().startswith(('play_', 'stop_')):
        src_names['dat.play_stop_strings'][w] += n
D = {}
hit_all = set()


def event_of(s):
    # epp PlaySound carries the event id itself as decimal; others carry names.
    return int(s) if s.isdigit() else fnv1_32(s)


for k, c in src_names.items():
    hits = [s for s in c if event_of(s) in ev]
    hit_all |= {event_of(s) for s in hits}
    D[k] = {'distinct': len(c), 'match_event': len(hits), 'uses': sum(c.values()),
            'uses_matching': sum(c[s] for s in hits),
            'miss_examples': [s for s, _ in c.most_common() if event_of(s) not in ev][:10]}
# Unprefixed dat identifiers that hash to an event id (chance rate reported).
other = [w for w in dat_words if not w.lower().startswith(('play_', 'stop_'))]
oh = [w for w in other if fnv1_32(w) in ev]
D['dat.other_identifiers'] = {'distinct': len(other), 'match_event': len(oh),
                              'expected_by_chance': round(len(other) * len(ev) / 2 ** 32, 2),
                              'examples': oh[:15]}
R['D_event_names'] = D

# E. action targets
objs = set(int(k) for k in W['sounds']) | ev | set(int(k) for k in W['actions'])
# all HIRC ids are not stored; count targets found among sounds/events/actions or flagged unknown
at = collections.Counter(); tgt_kind = collections.Counter()
for aid, (atype, tgt) in W['actions'].items():
    at[atype >> 8] += 1
    tgt_kind['sound' if str(tgt) in W['sounds'] else ('zero' if tgt == 0 else 'other-object')] += 1
R['E_actions'] = {'action_type_high_byte': dict(at), 'targets': dict(tgt_kind)}

# F. wem headers
fmt = collections.Counter()
for h, b in riff_heads.items():
    tag, ch, rate = struct.unpack_from('<HHI', b, 20)
    fmt[(hex(tag), ch, rate)] += 1
R['F_wem_fmt'] = {str(k): v for k, v in fmt.most_common()}
json.dump(R, open(out, 'w'), indent=1)
print(json.dumps(R, indent=1))
