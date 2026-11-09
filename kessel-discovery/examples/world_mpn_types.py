"""Cross mpnConditionEType (e986726f) against icon, quest-step listing, metadata target, placement.
usage: mpn_types.py <Phase.jsonl> <spice> <mapnotes.jsonl>"""
import collections, json, sqlite3, sys

db = sqlite3.connect('file:' + sys.argv[2] + '?mode=ro', uri=True)
listed = collections.Counter()
qkind = {}
for q, mn in db.execute("select quest_fqn, map_notes from quest_tasks where map_notes like '%mpn.%'"):
    for n in json.loads(mn):
        listed[n] += 1
placed = collections.Counter(json.loads(l)['template'] for l in open(sys.argv[3]))
kind_of = {}
by = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
ex = collections.defaultdict(list)
for l in open(sys.argv[1]):
    o = json.loads(l)
    v, f = o['v'], o['fqn']
    t = (v.get('e986726f') or {}).get('enum', 'none')
    d = by[t]
    d['icon'][v.get('7946402a')] += 1
    d['listed'][listed[f] > 0] += 1
    d['placed'][placed[f] > 0] += 1
    m = v.get('67688080')
    d['meta'][str(m).split('.')[0] if isinstance(m, str) else ('raw' if m is not None else None)] += 1
    d['metaid'][v.get('67688083') is not None] += 1
    d['cond'][v.get('9b7e5300') is not None] += 1
    d['link'][v.get('e4557d07') is not None] += 1
    d['prefix'][f.split('.')[1]] += 1
    if len(ex[t]) < 4:
        ex[t].append((f, m, v.get('67688082'), v.get('67688083')))
for t in sorted(by, key=lambda k: (str(type(k)), k)):
    d = by[t]
    n = sum(d['icon'].values())
    print('== enum', t, 'n', n)
    for k in ('icon', 'listed', 'placed', 'meta', 'metaid', 'cond', 'link', 'prefix'):
        print('  ', k, d[k].most_common(6))
    for e in ex[t]:
        print('   ex', e)
