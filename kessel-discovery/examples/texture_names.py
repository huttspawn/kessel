"""texture_names -- which DDS textures the client itself names, and what the
.tex sidecars and material inputs mean (#429 criterion 7, textures).

No dictionary. Path sources, each hashed the archive's way (hash_paths):
  * materials (.mat, root <Material>): every <input> of <type>texture</type>,
    '/resources/' + value + '.dds' (also DefaultPath / DefaultTextureObject);
  * effect specs: _fxProjectionTexture / _fxRampMap / _fxIllumMap values;
  * icons: kessel objects.icon_name -> /resources/gfx/icons/<lowercased>.dds;
  * .tex sidecar test: for every named DDS path p.dds, is p.tex a
    <TextureObject> entry?
Also tallies each material semantic against its texture-name suffix.

  python3 -I texture_names.py <mat_dir> <fxspec_resources.json> <spice.sqlite> <census.tsv> <hash_paths> <tex_dir>
"""
import collections, json, os, re, sqlite3, subprocess, sys
import xml.etree.ElementTree as ET

matd, fxres, spice, cen, hp, texd = sys.argv[1:7]
cls = {}
for l in open(cen, newline=''):
    a, h, comp, cs, us, c, head = l.rstrip('\n').split('\t')
    cls[h] = c


def hashp(paths):
    paths = sorted(set(paths))
    r = subprocess.run([hp], input='\n'.join(paths), capture_output=True, text=True).stdout
    return {p: h for h, p in (x.split('\t', 1) for x in r.splitlines())}


def norm(v):
    v = v.strip().replace('\\', '/').lower()
    v = re.sub(r'\.(dds|tex|tga|psd)$', '', v)
    return '/resources/' + v.lstrip('/') + '.dds'


src = collections.defaultdict(set)
sem = collections.defaultdict(collections.Counter)
tex_opts = collections.defaultdict(collections.Counter)
for fn in os.listdir(matd):
    b = open(os.path.join(matd, fn), 'rb').read()
    t = (b.decode('utf-16-le', 'replace') if (b[:2] == b'\xff\xfe' or b[1:2] == b'\0') else b.decode('utf-8', 'replace')).lstrip('﻿')
    try:
        r = ET.fromstring(t)
    except ET.ParseError:
        continue
    for inp in r.findall('input'):
        if (inp.findtext('type') or '') == 'texture' and (inp.findtext('value') or '').strip():
            v = inp.findtext('value'); src['material'].add(norm(v))
            m = re.search(r'_([a-z0-9]{1,3})$', v.strip().lower())
            sem[inp.findtext('semantic')]['_' + m.group(1) if m else '(none)'] += 1
    for tag in ('DefaultPath', 'DefaultTextureObject'):
        v = (r.findtext(tag) or '').strip()
        if v:
            src['material.' + tag].add(norm(v))
fx = json.load(open(fxres))
for k in ('_fxProjectorList', '_fxLightList'):
    pass
# fxspec_census.py writes every _fxResourceName per list; projector textures
# come from fxspec_links' own walk, so re-read the fxspec resource lists here
# only for names ending in .dds.
for ln, vals in fx.items():
    for v in vals:
        if v.lower().endswith('.dds'):
            src['fxspec'].add(norm(v))
db = sqlite3.connect(f'file:{spice}?mode=ro', uri=True)
for (n,) in db.execute("select distinct icon_name from objects where icon_name is not null and icon_name<>''"):
    src['icon'].add('/resources/gfx/icons/' + n.lower() + '.dds')
allp = set().union(*src.values())
H = hashp(allp | {p[:-4] + '.tex' for p in allp})
dds = {h for h, c in cls.items() if c == 'DDS'}
texe = {h for h, c in cls.items() if c == 'XML:TextureObject'}
named = set(); R = {'sources': {}}
for k, ps in src.items():
    hs = {H[p] for p in ps}
    R['sources'][k] = {'paths': len(ps), 'hit_dds': len(hs & dds)}
    named |= hs & dds
R['dds_entries'] = len(dds); R['dds_named_by_client_refs'] = len(named)
named_paths = [p for p in allp if H[p] in dds]
with_tex = sum(H[p[:-4] + '.tex'] in texe for p in named_paths)
R['named_dds_with_tex_sidecar'] = with_tex
R['tex_entries'] = len(texe)
R['tex_entries_reached_as_named_dds_sibling'] = len({H[p[:-4] + '.tex'] for p in named_paths} & texe)
R['material_semantic_suffix'] = {s: dict(c.most_common(5)) for s, c in sorted(sem.items(), key=lambda x: -sum(x[1].values()))[:16]}
# .tex sidecar field census
fields = collections.defaultdict(collections.Counter)
for fn in os.listdir(texd):
    b = open(os.path.join(texd, fn), 'rb').read()
    t = (b.decode('utf-16-le', 'replace') if (b[:2] == b'\xff\xfe' or b[1:2] == b'\0') else b.decode('utf-8', 'replace')).lstrip('﻿')
    try:
        r = ET.fromstring(t)
    except ET.ParseError:
        fields['PARSE-ERROR']['x'] += 1; continue
    for c in r:
        fields[c.tag][(c.text or '').strip()] += 1
R['tex_fields'] = {k: dict(v.most_common(8)) for k, v in fields.items()}
print(json.dumps(R, indent=1))
