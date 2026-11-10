"""Decode and check the 8.0 quest-reward format (qstRewardsMapping + qstRewardsSetMap).

usage: python3 -I pts_quest_rewards.py <old qstRewardsInfoPrototype.json> <new qstRewardsInfoPrototype.json> \
           <new unfiltered spice.sqlite> <cnd.trp field_values jsonl>

The two JSON files are dump_singleton output for the live (old format) and PTS (new format)
builds; the jsonl is `field_values <db> cnd.trp.` on the PTS extract. Prints:
- the mapping shape (quest -> qstBranchId -> reward-set key) and how many references resolve;
- old vs new reward lists per (quest, branch): identical once the level cap moves 80 -> 85,
  identical apart from quest gear replaced by Kalikoran archetype rows, or otherwise changed;
- the row field census and condition families;
- reward items absent from the build, named by the canonical GUID fold of FNV-1a 64 of a
  candidate FQN (Kalikoran and origin-story families with every ilvl and quality);
- orphan reward sets (referenced by no quest) and how many equal a retired live reward list;
- each loot-archetype condition's logic and the disciplines its IsLootDiscipline clauses name.
"""
import collections
import json
import sqlite3
import sys

M = (1 << 64) - 1
QUAL = ('premium', 'prototype', 'artifact', 'legendary', 'cheap', 'standard', 'moddable')


def fnv64(s):
    h = 0xCBF29CE484222325
    for b in s.upper().encode():
        h = ((h ^ b) * 0x100000001B3) & M
    return h


def fold(h):
    return 0xE000000000000000 | (h & 0x0000FFFFFFFF0000) | ((h ^ (h >> 48)) & 0xFFFF)


def sig(r):
    hi = r.get('b3006cb9')
    hi = 85 if hi == 80 else hi
    return (str(r.get('d9f35ee6')), str(r.get('07958075')), r.get('b3006cb8'), hi, bool(r.get('d9f35ee5')),
            json.dumps(r.get('d9f35ee9'), sort_keys=True), str(r.get('c7880469')), str(r.get('c788046b')))


def main():
    old_path, new_path, db_path, cnd_path = sys.argv[1:]
    old, new = json.load(open(old_path)), json.load(open(new_path))
    mapping = {e['k']: {x['k']: x['v'] for x in e['v']} for e in new['499053b3']}
    sets = {e['k']: e['v'] for e in new['499053b2']}
    refs = [v for m in mapping.values() for v in m.values()]
    print('quests', len(mapping), 'set refs', len(refs), 'resolve', sum(r in sets for r in refs),
          'distinct sets used', len(set(refs)), 'sets', len(sets))
    print('branch keys', collections.Counter(k for m in mapping.values() for k in m))

    og = {e['k']: e['v'] for e in old['e146d0c0']}
    ob = {e['k']: {x['k']: x['v'] for x in e['v']} for e in old['c788046a']}
    print('old branch keys reproduced', all(set(ob[q]) <= set(mapping.get(q, {})) for q in ob))
    cat = collections.Counter()
    for q, rows, b in [(q, og[q], 0) for q in og] + [(q, r, b) for q, m in ob.items() for b, r in m.items()]:
        if q not in mapping or b not in mapping[q]:
            cat['quest or branch dropped'] += 1
            continue
        a = sorted(map(sig, rows))
        n = sorted(map(sig, sets[mapping[q][b]]))
        if a == n:
            cat['same after level cap 80 -> 85'] += 1
            continue
        keep = lambda rs, drop_motf: sorted(sig(r) for r in rs if not str(r.get('d9f35ee6')).startswith('@itm.gen.quest')
                                            and not (drop_motf and str(r.get('d9f35ee6')).startswith('@itm.gen.motf')))
        cat['quest gear replaced by Kalikoran rows' if keep(rows, False) == keep(sets[mapping[q][b]], True) else 'other change'] += 1
    print('old -> new reward lists', dict(cat))
    rows = [r for s in sets.values() for r in s]
    print('rows', len(rows), 'fields', dict(collections.Counter(k for r in rows for k in r)))
    print('conditions per row', dict(collections.Counter(len(r['1a7c1f02']) for r in rows)))
    print('condition families', collections.Counter('.'.join(str(c).split('.')[:3]) for r in rows for c in r['1a7c1f02']).most_common(12))

    con = sqlite3.connect(f'file:{db_path}?mode=ro', uri=True)
    guid = {int(g, 16): f for f, g in con.execute('select fqn, guid from objects')}
    missing = collections.Counter(r['d9f35ee6'] for r in rows if not isinstance(r['d9f35ee6'], str))
    print('rows naming an item absent from the build', sum(missing.values()), 'distinct', len(missing),
          'any in objects', sum(1 for g in missing if g in guid))
    tmpl = set()
    for (f,) in con.execute("select fqn from objects where kind = 'Item' and (fqn like 'itm.gen.motf.%' or fqn like 'itm.gen.quest.origin_story.%')"):
        p = ['{IL}' if t.startswith('ilvl_') else ('{Q}' if t in QUAL else t) for t in f.split('.')]
        tmpl.add('.'.join(p))
        tmpl.add('.'.join(p).replace('.dps.', '.tank.').replace('.heals.', '.tank.'))
    named = {}
    for t in tmpl:
        for il in range(1, 200):
            x = t.replace('{IL}', f'ilvl_{il:04d}')
            for q in (QUAL if '{Q}' in x else ('',)):
                name = x.replace('{Q}', q)
                g = fold(fnv64(name))
                if g in missing:
                    named[g] = name
    print('absent items named', len(named), 'of', len(missing), collections.Counter('.'.join(n.split('.')[:4]) for n in named.values()))

    orphan = set(sets) - set(refs)
    oldsigs = {tuple(sorted(map(sig, v))) for v in og.values()} | {tuple(sorted(map(sig, r))) for m in ob.values() for r in m.values()}
    print('orphan sets', len(orphan), 'equal to a live reward list', sum(1 for k in orphan if tuple(sorted(map(sig, sets[k]))) in oldsigs))

    cnds = [json.loads(l) for l in open(cnd_path) if l.startswith('{')]
    for c in cnds:
        if c['fqn'].startswith('cnd.trp.loot_archetype.') and '.weapons.' not in c['fqn']:
            logic = 'OR' if (c.get('9e352b11') or {}).get('enum') == 1 else 'AND'
            dis = [guid.get(cl.get('3a9c8930'), cl.get('3a9c8930')) for cl in c['04ee5c1e'] if cl['04ee5c20']['enum'] == 86]
            print(c['fqn'], logic, len(dis), dis)


if __name__ == '__main__':
    main()
