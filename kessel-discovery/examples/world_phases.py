"""Census of phs.* (real phase) objects. usage: phs.py <phs.jsonl> <spice> <areas_dir> <missing18>"""
import collections, json, os, sqlite3, sys

db = sqlite3.connect('file:' + sys.argv[2] + '?mode=ro', uri=True)
dirs = {int(d) for d in os.listdir(sys.argv[3])}
missing = {int(x) for x in open(sys.argv[4]).read().split()}
fields = collections.Counter()
samples = collections.defaultdict(list)
area_hit = collections.Counter()
miss_hits = collections.defaultdict(list)
scripts = []
for l in open(sys.argv[1]):
    o = json.loads(l)
    v = o['v']
    for k, x in v.items():
        fields[k] += 1
        if len(samples[k]) < 3:
            samples[k].append(x)
    a = v.get('7f9e4ac0')
    if a is not None:
        area_hit['dir' if a in dirs else 'not-a-dir'] += 1
        if a in missing:
            miss_hits[a].append(o['fqn'])
    s = v.get('4e63e81d')
    if isinstance(s, str):
        scripts.append(s.lstrip('@'))
print('phs objects, field counts')
for k, n in fields.most_common():
    print(' ', n, k, json.dumps(samples[k])[:200])
print('7f9e4ac0 vs area dirs', area_hit)
print('distinct area dirs a phase names', len({json.loads(l)['v'].get('7f9e4ac0') for l in open(sys.argv[1])} & dirs))
print('missing18 named by a phase area field:', {k: v[:3] for k, v in miss_hits.items()})
h = collections.Counter()
acts = collections.Counter()
for s in scripts:
    for hd, a in db.execute("select handler, action from script_actions where script_fqn=?", (s,)):
        h[hd] += 1
        acts[a] += 1
print('phase scripts', len(scripts), 'with actions', sum(1 for s in scripts if db.execute("select 1 from script_actions where script_fqn=? limit 1", (s,)).fetchone()))
print('handlers', h.most_common(15))
print('actions', acts.most_common(25))
print('quest_phases rows', db.execute("select count(*), count(distinct phase_fqn), min(phase_fqn) from quest_phases").fetchone())
