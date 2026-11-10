"""GSF chassis stats: the pkg.spvp.<class> packages' STAT-keyed base values, with STAT names.

usage: python3 -I gsf_ship_stats.py <pkg.jsonl> [gom_enums.json]

pkg.jsonl: dump_kind_jsonl <unfiltered extract> pkg. Each pkg.spvp.<class> carries e133970d, a map
STAT enum -> float. The ship records in scFFShipsDataPrototype point at their package through
e133970b (@pkg.spvp.striker ...). gom_enums.json defaults to this repo's kessel/resources copy.
Prints one row per STAT with the value for each class package.
"""
import json, os, sys

PKG = sys.argv[1]
ENUMS = sys.argv[2] if len(sys.argv) > 2 else os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'kessel', 'resources', 'gom_enums.json')
STAT = next(e for e in json.load(open(ENUMS)) if e['name'] == 'STAT')['members']
pk = {}
for line in open(PKG):
    o = json.loads(line)
    if o['fqn'].startswith('pkg.spvp.') and o['fqn'].count('.') == 2:
        pk[o['fqn'].split('.')[-1]] = {e['k']['enum']: e['v'] for e in o['v'].get('e133970d', [])}
cls = sorted(pk)
print('stat'.ljust(62), ' '.join(c[:11].rjust(11) for c in cls))
for s in sorted({s for v in pk.values() for s in v}):
    name = STAT[s] if s < len(STAT) else f'#{s}'
    print(f'{s:4d} {name[:56]:56s}', ' '.join(f"{pk[c].get(s, ''):>11}" if not isinstance(pk[c].get(s), float) else f'{pk[c][s]:11.4g}' for c in cls))
