"""appearance_paths -- decode push 3 (#429), media group: how an item or NPC
appearance names its model, material and texture files, proved by join.

The chain, every link read from the client:
  ipp / npp / pcs appearance slot (GOM, dumped by dump_kind_jsonl)
    appAppearanceSlotType (16c18126, enum)       -> slot index file
    appAppearanceSlotModelID (16c18127)          -> <Asset><ID> in that slot's index.xml
    appAppearanceSlotMaterialIndex (16c18128)    -> <Material id> inside that Asset
    appAppearanceSlotAttachments (16c18129)      -> <Attachment id> inside that Asset
    appAppearanceSlotHuePrimary/Secondary (2a/2b)-> garmenthue index <Asset><ID>
    ippColorScheme (cf4008a8)                    -> colorscheme index <Asset><ID>
  index.xml BaseFile / Attachment filename       -> .gr2 path with [bt] = body type
  Material filename                              -> .mat XML -> texture paths (.dds)
/resources/art/dynamic/index.xml names every slot index file, so no path in
the chain comes from a dictionary.

Usage:
  python3 -I appearance_paths.py <dynamic dir> <jsonl dir> <census.tsv> <out dir> [mat dir]
    <dynamic dir>  extracted /resources/art/dynamic (index.xml + <slot>/index.xml)
    <jsonl dir>    ItemPaintPattern.jsonl, NpcPackage.jsonl, CharacterPreset.jsonl
    <census.tsv>   media_census output (every archive entry hash)
    <out dir>      writes mat_paths.txt (to extract), report.json
    [mat dir]      extracted .mat files; when given, resolves textures too
"""
import collections
import json
import os
import sys
import xml.etree.ElementTree as ET

M32 = 0xFFFFFFFF


def rot(x, k):
    return ((x << k) | (x >> (32 - k))) & M32


def hashlittle2(key: bytes):
    """Jenkins lookup3 hashlittle2(key, 0, 0); archive hash = (b << 32) | c."""
    n = len(key)
    a = b = c = (0xDEADBEEF + n) & M32
    i = 0
    while n > 12:
        a = (a + int.from_bytes(key[i:i + 4], 'little')) & M32
        b = (b + int.from_bytes(key[i + 4:i + 8], 'little')) & M32
        c = (c + int.from_bytes(key[i + 8:i + 12], 'little')) & M32
        a = (a - c) & M32; a ^= rot(c, 4); c = (c + b) & M32
        b = (b - a) & M32; b ^= rot(a, 6); a = (a + c) & M32
        c = (c - b) & M32; c ^= rot(b, 8); b = (b + a) & M32
        a = (a - c) & M32; a ^= rot(c, 16); c = (c + b) & M32
        b = (b - a) & M32; b ^= rot(a, 19); a = (a + c) & M32
        c = (c - b) & M32; c ^= rot(b, 4); b = (b + a) & M32
        i += 12
        n -= 12
    if n == 0:
        return (b << 32) | c
    tail = key[i:i + n] + b'\0' * (12 - n)
    a = (a + int.from_bytes(tail[0:4], 'little')) & M32
    b = (b + int.from_bytes(tail[4:8], 'little')) & M32
    c = (c + int.from_bytes(tail[8:12], 'little')) & M32
    c ^= b; c = (c - rot(b, 14)) & M32
    a ^= c; a = (a - rot(c, 11)) & M32
    b ^= a; b = (b - rot(a, 25)) & M32
    c ^= b; c = (c - rot(b, 16)) & M32
    a ^= c; a = (a - rot(c, 4)) & M32
    b ^= a; b = (b - rot(a, 14)) & M32
    c ^= b; c = (c - rot(b, 24)) & M32
    return (b << 32) | c


def res(path: str) -> str:
    """index.xml and .mat paths are relative to /resources, may use backslashes
    and mixed case; archive keys hash the lower-case path."""
    p = path.strip().replace('\\', '/').lower()
    if not p.startswith('/'):
        p = '/' + p
    return p if p.startswith('/resources/') else '/resources' + p


BODY_TYPES = ['bfa', 'bfb', 'bfn', 'bfs', 'bma', 'bmf', 'bmn', 'bms']


def expand(f, bt):
    """Fill the path macros the body-type spec (.dyc [PARTSMACROS]) defines:
    [bt] the body type, [gen] its gender letter (bfa -> f, bma -> m)."""
    g = bt[1] if len(bt) == 3 and bt[0] == 'b' else ''
    return f.replace('[bt]', bt).replace('[gen]', g)


def load_indexes(dyn):
    top = ET.parse(os.path.join(dyn, 'index.xml')).getroot()
    slots = {}
    for f in top.findall('AssetIndexFile'):
        rel = f.text.strip().replace('/art/dynamic/', '', 1)
        p = os.path.join(dyn, rel)
        assets = {}
        for a in ET.parse(p).getroot().findall('Asset'):
            aid = int(a.findtext('ID'))
            assets[aid] = {
                'art': a.findtext('ArtName'),
                'base': (a.findtext('BaseFile') or '').strip(),
                'att': {int(x.get('id')): x.get('filename') for x in a.iter('Attachment')},
                'mat': {int(x.get('id')): x.get('filename') for x in a.iter('Material') if x.get('id')},
                'bt': [x.text for x in a.iter('Bodytype')],
                'skin': [x.get('filename') for x in a.iter('SkinMaterial')],
                'ovr': [x.get('filename') for x in a.iter('MaterialOverride')],
            }
        slots[f.get('slot')] = assets
    return slots


def slot_records(obj):
    """Yield every appearance-slot struct (dict holding 16c18126) in an object."""
    if isinstance(obj, dict):
        if '16c18126' in obj:
            yield obj
        for v in obj.values():
            yield from slot_records(v)
    elif isinstance(obj, list):
        for v in obj:
            yield from slot_records(v)


def main():
    dyn, jdir, census, out = sys.argv[1:5]
    matdir = sys.argv[5] if len(sys.argv) > 5 else None
    os.makedirs(out, exist_ok=True)
    slots = load_indexes(dyn)
    present = set()
    for line in open(census):
        p = line.split('\t')
        if len(p) >= 2:
            try:
                present.add(int(p[1], 16))
            except ValueError:
                pass
    rep = {'slots': {s: len(a) for s, a in slots.items()}, 'archive_entries': len(present)}

    # hashing self-check: index.xml files themselves must be present
    hits = sum(hashlittle2(f'/resources/art/dynamic/{s}/index.xml'.encode()) in present for s in slots)
    rep['index_files_present'] = f'{hits}/{len(slots)}'

    # Every path the indexes name, with [bt] expanded over the asset's body types
    gr2_paths, mat_paths = set(), set()
    for s, assets in slots.items():
        for a in assets.values():
            bts = a['bt'] or ['']
            for f in [a['base']] + list(a['att'].values()):
                if f and f.endswith('.gr2'):
                    for bt in bts:
                        gr2_paths.add(res(expand(f, bt)))
            for f in list(a['mat'].values()) + a['skin'] + a['ovr']:
                if f and f.endswith('.mat'):
                    for bt in bts:
                        mat_paths.add(res(expand(f, bt)))
    g_hit = sum(hashlittle2(p.encode()) in present for p in gr2_paths)
    m_hit = [p for p in mat_paths if hashlittle2(p.encode()) in present]
    rep['index_gr2_paths'] = {'named': len(gr2_paths), 'in_archive': g_hit}
    rep['index_mat_paths'] = {'named': len(mat_paths), 'in_archive': len(m_hit)}
    with open(os.path.join(out, 'mat_paths.txt'), 'w') as fh:
        fh.write('\n'.join(sorted(m_hit)) + '\n')

    # Join every appearance slot of every ipp / npp / pcs object
    enum_slot = collections.defaultdict(collections.Counter)
    recs = []
    for kind in ['ItemPaintPattern', 'NpcPackage', 'CharacterPreset']:
        path = os.path.join(jdir, kind + '.jsonl')
        if not os.path.exists(path):
            continue
        for line in open(path):
            o = json.loads(line)
            for r in slot_records(o['v']):
                recs.append((kind, o['fqn'], r))
                mid = r.get('16c18127')
                e = r['16c18126'].get('enum') if isinstance(r['16c18126'], dict) else None
                if isinstance(mid, int):
                    for s, assets in slots.items():
                        if mid in assets:
                            enum_slot[e][s] += 1
    # an enum value names the slot index that holds its model ids
    emap = {e: c.most_common(1)[0][0] for e, c in enum_slot.items() if c}
    rep['slot_enum'] = {str(e): {'slot': emap[e], 'votes': dict(enum_slot[e])} for e in sorted(emap)}

    stats = collections.defaultdict(collections.Counter)
    hue = slots.get('garmenthue', {})
    cs = slots.get('colorscheme', {})
    for kind, fqn, r in recs:
        st = stats[kind]
        st['slots'] += 1
        e = r['16c18126'].get('enum') if isinstance(r['16c18126'], dict) else None
        s = emap.get(e)
        mid = r.get('16c18127')
        if mid is None:
            st['no_model_id'] += 1
            continue
        a = slots.get(s, {}).get(mid)
        if a is None:
            st['model_unresolved'] += 1
            continue
        st['model_resolved'] += 1
        mi = r.get('16c18128')
        if mi is not None:
            if mi in a['mat']:
                st['material_resolved'] += 1
            elif mi == -1:
                st['material_none(-1)'] += 1
            else:
                st['material_unresolved'] += 1
        for at in r.get('16c18129') or []:
            st['attachment_resolved' if at in a['att'] else 'attachment_unresolved'] += 1
        for f in ('16c1812a', '16c1812b'):
            if f in r:
                st['hue_resolved' if r[f] in hue else 'hue_unresolved'] += 1
        # the model file exists for at least one body type
        if a['base']:
            ok = any(hashlittle2(res(expand(a['base'], bt)).encode()) in present
                     for bt in (a['bt'] or ['']))
            st['model_file_present' if ok else 'model_file_absent'] += 1
        else:
            st['model_no_basefile'] += 1
    rep['joins'] = {k: dict(v) for k, v in stats.items()}
    cc = collections.Counter()
    for kind in ['ItemPaintPattern']:
        path = os.path.join(jdir, kind + '.jsonl')
        if os.path.exists(path):
            for line in open(path):
                v = json.loads(line)['v'].get('cf4008a8')
                if v is not None:
                    cc['colorscheme_resolved' if v in cs else 'colorscheme_unresolved'] += 1
    rep['ipp_colorscheme'] = dict(cc)

    if matdir:
        # matdir holds every archive Material XML as <HASH>.bin (media_census dump);
        # each .mat the indexes name is opened by its path hash
        tex = collections.Counter()
        sem = collections.Counter()
        texpaths = set()
        for p in m_hit:
            fp = os.path.join(matdir, f'{hashlittle2(p.encode()):016X}.bin')
            if not os.path.exists(fp):
                tex['mat_not_in_dump'] += 1
                continue
            try:
                t = ET.fromstring(open(fp, 'rb').read().decode('utf-8-sig'))
            except (ET.ParseError, UnicodeDecodeError):
                tex['mat_parse_error'] += 1
                continue
            tex['mat_parsed'] += 1
            for inp in t.iter('input'):
                if (inp.findtext('type') or '').strip() == 'texture':
                    v = (inp.findtext('value') or '').strip()
                    if v:
                        sem[inp.findtext('semantic')] += 1
                        texpaths.add(v)
        hit = collections.Counter()
        for v in texpaths:
            p = res(v)
            dds = hashlittle2((p + '.dds').encode()) in present
            texo = hashlittle2((p + '.tex').encode()) in present
            hit['dds' if dds else ('tex only' if texo else 'neither')] += 1
        tex['texture_refs_distinct'] = len(texpaths)
        tex['texture_refs_resolution'] = dict(hit)
        rep['texture_semantics'] = dict(sem.most_common(20))
        rep['materials'] = dict(tex)
    json.dump(rep, open(os.path.join(out, 'report.json'), 'w'), indent=1)
    print(json.dumps(rep, indent=1))


if __name__ == '__main__':
    main()
