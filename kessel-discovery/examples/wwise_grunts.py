"""wwise_grunts -- how an NPC's combat grunts reach audio (#429 criterion 7).

Chain proved here:
  epp PlayGrunt sndGruntEvent 'sndGruntEvt_<Slot>'
    -> Wwise event 'Play_voc_<Slot>' (FNV-1 32, lowercased) in bank voc_all
    -> one of voc_all's SwitchCntrs, whose switch values are soundset names
       (FNV-1 32 of '<soundset>', the bank name minus 'voc_')
    -> the soundset bank voc_<soundset> (bank id = FNV-1 32 of its name)
  and the NPC names its soundset as a '<name>_soundset' string in its npc/npp
  payload (kessel objects.json 'strings').

  python3 -I wwise_grunts.py <wwise.json> <bank_dir> <epp_dir> <spice.sqlite>
"""
import collections, json, os, re, sqlite3, struct, sys
import xml.etree.ElementTree as ET

wj, bdir, eppd, spice = sys.argv[1:5]
W = json.load(open(wj))


def fnv(s):
    h = 0x811C9DC5
    for c in s.lower().encode():
        h = (h * 0x01000193) & 0xFFFFFFFF
        h ^= c
    return h


names = W['bank_names']
ev = {int(e) for e in W['events']}
fn = next(k for k, v in W['per_bank'].items() if names.get(str(v['bank_id'])) == 'voc_all')
b = open(os.path.join(bdir, fn), 'rb').read()
p = b.find(b'HIRC'); size = struct.unpack_from('<I', b, p + 4)[0]; hirc = b[p + 8:p + 8 + size]
u32 = collections.Counter(struct.unpack_from('<I', hirc, i)[0] for i in range(0, len(hirc) - 3))
n_switch = W['per_bank'][fn]['hirc'].get('SwitchCntr', 0)

slots = collections.Counter()
for f in os.listdir(eppd):
    t = open(os.path.join(eppd, f), 'rb').read()
    t = (t.decode('utf-16-le', 'replace') if (t[:2] == b'\xff\xfe' or t[1:2] == b'\0') else t.decode('utf-8', 'replace')).lstrip('﻿')
    try:
        r = ET.fromstring(t)
    except ET.ParseError:
        continue
    for x in r.iter('sndGruntEvent'):
        if (x.text or '').strip():
            slots[x.text.strip()] += 1
slot_hit = {s: fnv('Play_voc_' + s.split('sndGruntEvt_', 1)[-1]) in ev for s in slots}

soundset_banks = {n[4:] for n in names.values() if n.startswith('voc_')}
as_switch = {s for s in soundset_banks if u32[fnv(s)] >= 1}
db = sqlite3.connect(f'file:{spice}?mode=ro', uri=True)
npc_sets = collections.Counter()
for (j,) in db.execute("select json from objects where json like '%_soundset%'"):
    for s in json.loads(j).get('strings') or []:
        if isinstance(s, str) and s.endswith('_soundset'):
            npc_sets[s.rsplit('.', 1)[-1]] += 1  # 'chr.<set>' and bare '<set>'
npc_with_bank = {s for s in npc_sets if s in soundset_banks}
npc_in_switch = {s for s in npc_sets if u32[fnv(s)] >= 1}
R = {'grunt_slots': len(slots), 'grunt_slot_uses': sum(slots.values()),
     'slots_with_Play_voc_event': sum(slot_hit.values()),
     'slot_misses': [s for s, ok in slot_hit.items() if not ok],
     'voc_all_switch_containers': n_switch,
     'voc_bank_names': len(soundset_banks), 'voc_names_used_as_voc_all_switch_values': len(as_switch),
     'switch_value_occurrences_per_name': dict(collections.Counter(u32[fnv(s)] for s in as_switch)),
     'npc_soundset_strings': len(npc_sets), 'npc_soundsets_with_bank': len(npc_with_bank),
     'npc_soundsets_in_switch': len(npc_in_switch),
     'npc_soundsets_missing_bank': sorted(set(npc_sets) - npc_with_bank)[:20]}
print(json.dumps(R, indent=1))
