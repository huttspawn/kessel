"""Cross the trigger volume type enum (80af3c1a, stored as trigger_volumes.shape) with what the volume names.
usage: a9_shape.py <spice> <unfiltered>"""
import collections, sqlite3, sys

db = sqlite3.connect('file:' + sys.argv[1] + '?mode=ro', uri=True)
un = sqlite3.connect('file:' + sys.argv[2] + '?mode=ro', uri=True)
phs = {f.split('.', 1)[1] for (f,) in un.execute("select distinct fqn from objects where kind='phs'")}
NAMES = ['GENERIC', 'BARK', 'INSTANCE_REGION', 'INSTANCE_GATEWAY', 'QUEST_REGION', 'HYDRA', 'BARRIER', 'AUDIO_REGION',
         'MAP', 'COMPANION_EQUIPMENT', 'COMPANION_PRIVATE', 'DEATH_VOLUME', 'EXHAUSTION_VOLUME', 'BLOCKING_VOLUME',
         'FLAGSHIP_TRANSPORT_BLOCKING_VOLUME', 'CONTROL_POINT_VOLUME', 'WORLD_QUEST_VOLUME',
         'SHARED_WORLD_QUEST_VOLUME', 'META_WORLD_QUEST_VOLUME', 'PLANETARY_WORLD_QUEST_VOLUME']
t = collections.defaultdict(collections.Counter)
ex = collections.defaultdict(list)
for shape, scr, reg, mp in db.execute(
        "select v.shape, v.script_fqn, v.region_name, 0 from trigger_volumes v"):
    if scr:
        what = 'hyd script'
    elif reg:
        what = 'phs name' if reg in phs else 'other name'
    else:
        what = 'nothing'
    t[shape][what] += 1
    if reg and reg not in phs and len(ex[shape]) < 6 and reg not in ex[shape]:
        ex[shape].append(reg)
for s in sorted(t, key=lambda x: (x is None, x)):
    name = NAMES[s - 1] if isinstance(s, int) and 1 <= s <= len(NAMES) else '?'
    print(s, name, dict(t[s]), 'other-name examples:', ex[s])
