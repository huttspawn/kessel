"""palette_fields -- decode push 3 (#429): where skin, hair, eye and garment
colours live, and what each palette field means.

The skin/hair/eye colour BaseFile XMLs the dynamic index.xml files name are
not in any archive. The values are in GOM objects 'dynamic.<kind>.<ArtName>'
(1,152 of them: skincolor, haircolor, eyecolor, garmenthue), decoded with
dump_kind_jsonl <unfiltered.sqlite> dynamic. Garment hues also ship as
<Palette> XML files; this tool matches every GOM garmenthue object against
its XML field by field and prints, per GOM field, which XML element it equals.

Usage: python3 -I palette_fields.py <dynamic.jsonl> <dir of garmenthue xml>
       <dir holding the dynamic */index.xml>
"""
import collections
import json
import os
import sys
import xml.etree.ElementTree as ET

RGBA = ['e101edda', 'e101eddb', 'e101eddc', 'e101eddd']


def vec(v, default=0.0):
    """A GOM colour {r,g,b,a hash: f} (absent component = 0) or a list."""
    if isinstance(v, list):
        return [float(x) for x in v]
    return [float(v.get(k, default)) for k in RGBA]


def xml_fields(path):
    t = ET.parse(path).getroot()
    out = {}
    for el in t:
        s = (el.text or '').strip()
        try:
            out[el.tag] = [float(x) for x in s.split(',')]
        except ValueError:
            pass
    return out


def main():
    jl, xdir, idir = sys.argv[1:4]
    objs = {}
    kinds = collections.Counter()
    for line in open(jl):
        o = json.loads(line)
        objs[o['fqn']] = o['v']
        kinds[o['fqn'].split('.')[1]] += 1
    print('GOM dynamic objects by kind:', dict(kinds))
    votes = collections.defaultdict(collections.Counter)
    matched = 0
    for fn in os.listdir(xdir):
        name = fn[:-4]
        g = objs.get('dynamic.garmenthue.' + name)
        if g is None:
            continue
        matched += 1
        x = xml_fields(os.path.join(xdir, fn))
        for k, v in g.items():
            gv = vec(v) if not isinstance(v, (int, float)) else [float(v)]
            for tag, xv in x.items():
                if len(xv) == len(gv) and all(abs(a - b) < 1e-4 for a, b in zip(gv, xv)):
                    votes[k][tag] += 1
    print('garmenthue XML files with a GOM object:', matched, 'of', len(os.listdir(xdir)))
    for k in sorted(votes):
        print(f'  {k}: {votes[k].most_common(4)}')
    # every colour asset an index names has its GOM object
    for kind in ('skincolor', 'haircolor', 'eyecolor', 'garmenthue'):
        p = os.path.join(idir, kind, 'index.xml')
        names = [a.findtext('ArtName') for a in ET.parse(p).getroot().iter('Asset')]
        have = sum(('dynamic.%s.%s' % (kind, n)) in objs for n in names)
        print(f'{kind}: index assets {len(names)}, with a GOM dynamic object {have}')


if __name__ == '__main__':
    main()
