"""Join exhaustion/damage trigger-volume profile names to the environmental abilities that apply them.

usage: python3 -I volume_profile_abilities.py <spice.sqlite> <unfiltered.sqlite>

spice.sqlite: a default extract (trigger_volumes.region_name). unfiltered.sqlite: an unfiltered
extract (objects.fqn of kind Ability). Rule tried: dam_<x> -> an ability whose last segment is
damage_<x>; exh_<x> -> exhaustion_<x>; any other name -> an ability whose last segment equals it.
Prints coverage by volume and by distinct name, and the names that match nothing.
"""
import sqlite3, sys, collections

spice, unf = sys.argv[1], sys.argv[2]
names = collections.Counter(r[0] for r in sqlite3.connect(spice).execute(
    "select region_name from trigger_volumes where region_name is not null and region_name != '' and (script_fqn is null or script_fqn = '')"))
last = collections.defaultdict(set)
for (f,) in sqlite3.connect(unf).execute("select distinct fqn from objects where kind='Ability'"):
    last[f.rsplit('.', 1)[-1]].add(f)


def cands(n):
    if n.startswith('dam_'):
        rest = n[4:]
        out = set(last.get('damage_' + rest, set()))
        if '_' in rest:  # dam_<category>_<name> -> ...damage.<category>.damage_<name>
            cat, nm = rest.split('_', 1)
            out |= {f for f in last.get('damage_' + nm, set()) if f'.{cat}.' in f or '.location.' in f}
        return out
    if n.startswith('exh_'):
        rest = n[4:]
        return set(last.get('exhaustion_' + rest, set())) | set(last.get('exhaustion_' + rest.split('_')[0], set()))
    return set(last.get(n, set()))


prof = {n: c for n, c in names.items() if n.startswith(('dam_', 'exh_')) or 'falling' in n or 'death' in n}
hit = {n: c for n, c in prof.items() if cands(n)}
print('profile-shaped names', len(prof), 'on', sum(prof.values()), 'volumes; matched', len(hit), 'names on', sum(hit.values()), 'volumes')
for n, c in sorted(hit.items(), key=lambda x: -x[1])[:15]:
    print('  ', c, n, '->', sorted(cands(n))[:2])
print('unmatched:', sorted(((c, n) for n, c in prof.items() if n not in hit), reverse=True)[:25])
