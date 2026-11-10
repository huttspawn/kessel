"""fxspec_links -- prove what the names inside an effect spec point at
(#429 criterion 7). Three joins, none using the dictionary:

 1. Internal graph: every _fxStartFxName / _fxStopFxName / _fxOwnerName of an
    element resolves to the _fxName of an element of the same spec, or 'Root'.
 2. Asset paths: every _fxResourceName with a file extension (and projector
    textures, light ramp/illum maps) becomes /resources/<path>, hashed the
    archive's way (hash_paths), and is looked up in the content census.
 3. Sound events: every 3D/sound _fxResourceName (and _fxAudioStart/StopEvent)
    is split on ',' and hashed with Wwise's FNV-1 32 over the lowercased name;
    the id is looked up among the HIRC Event ids of every bank (wwise_banks).

  python3 -I fxspec_links.py <fxspec_dir> <census.tsv> <wwise.json> <hash_paths bin> <out.json>
"""
import collections, json, os, subprocess, sys
import xml.etree.ElementTree as ET

src, cen, wj, hp, out = sys.argv[1:6]


def text(b):
    if b[:2] == b'\xff\xfe' or (len(b) > 1 and b[1] == 0):
        return b.decode('utf-16-le', 'replace').lstrip('﻿')
    return b.decode('utf-8', 'replace').lstrip('﻿')


def fnv1_32(s):
    h = 0x811C9DC5
    for c in s.lower().encode():
        h = (h * 0x01000193) & 0xFFFFFFFF
        h ^= c
    return h


census = {}
for l in open(cen, newline=''):
    a, h, comp, cs, us, cls, head = l.rstrip('\n').split('\t')
    census[h] = cls
W = json.load(open(wj))
event_ids = {int(k) for k in W['events']}

graph = collections.Counter(); unresolved = collections.Counter()
paths = collections.defaultdict(collections.Counter)   # list -> path -> uses
sounds = collections.Counter(); sound_lists = collections.Counter()
files = 0
ASSET_FIELDS = ('_fxResourceName', '_fxProjectionTexture', '_fxRampMap', '_fxIllumMap')
for fn in sorted(os.listdir(src)):
    root = ET.fromstring(text(open(os.path.join(src, fn), 'rb').read()))
    files += 1
    elems = []
    for f in root.iter('f'):
        if f.get('name', '').endswith('List'):
            for e in f.findall('e'):
                elems.append((f.get('name'), {g.get('name'): (g.text or '').strip() for g in e.findall('f')}))
    names = {d.get('_fxName', '') for _, d in elems} | {'Root'}
    for ln, d in elems:
        for k in ('_fxStartFxName', '_fxStopFxName', '_fxOwnerName'):
            v = d.get(k)
            if v is None or v == '':
                continue
            if v in names:
                graph[(k, 'resolved')] += 1
            else:
                graph[(k, 'unresolved')] += 1; unresolved[v] += 1
        if ln in ('_fx3DSoundList', '_fxSoundList'):
            for k in ('_fxResourceName', '_fxAudioStartEvent', '_fxAudioStopEvent'):
                for s in (d.get(k) or '').replace('//', '').split(','):
                    s = s.strip()
                    if s and '.' not in s:
                        sounds[s] += 1; sound_lists[(ln, k)] += 1
        for k in ASSET_FIELDS:
            v = (d.get(k) or '').strip()
            if '.' in v.rsplit('\\', 1)[-1].rsplit('/', 1)[-1]:
                p = v.replace('\\', '/').lower()
                if not p.startswith('/'):
                    p = '/' + p
                if not p.startswith('/art') and not p.startswith('/engine') and not p.startswith('/audio'):
                    p = '/art/fx/particles' + p  # bare particle names
                paths[f'{ln}.{k}'][p] += 1

allp = sorted({p for c in paths.values() for p in c})
res = subprocess.run([hp], input='\n'.join('/resources' + p for p in allp), capture_output=True, text=True).stdout
ph = {}
for l in res.splitlines():
    h, p = l.split('\t', 1); ph[p[len('/resources'):]] = h
asset = {}
for key, c in paths.items():
    found = collections.Counter(); miss = []
    for p, n in c.items():
        cls = census.get(ph.get(p, ''))
        if cls:
            found[cls] += 1
        else:
            found['MISSING'] += 1
            if len(miss) < 8: miss.append(p)
    asset[key] = {'distinct': len(c), 'found_by_class': dict(found), 'missing_examples': miss}

hit = {s for s in sounds if fnv1_32(s) in event_ids}
miss = sorted((s for s in sounds if s not in hit), key=lambda s: -sounds[s])
R = {'files': files,
     'graph': {f'{k}:{r}': n for (k, r), n in graph.items()},
     'unresolved_top': unresolved.most_common(15),
     'assets': asset,
     'sound_names': len(sounds), 'sound_name_uses': sum(sounds.values()),
     'sound_names_matching_bank_event': len(hit),
     'sound_uses_matching': sum(sounds[s] for s in hit),
     'sound_fields': {f'{a}.{b}': n for (a, b), n in sound_lists.items()},
     'sound_miss_examples': miss[:40]}
json.dump(R, open(out, 'w'), indent=1)
print(json.dumps(R, indent=1))
