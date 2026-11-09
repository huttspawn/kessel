"""Check the story spine leg by leg: is each waypoint reachable from the previous one?

usage: python3 -I story_spine_legs.py <outdir>
Reads edges.tsv and quests.tsv (story_spine.py); writes legs.txt.

The waypoints are the ship mission console's storyline entries in the order their
own gates put them (console.txt), plus each storyline's exit (the quest that gates
the next entry). A leg is searched breadth-first over requires, handoff and script
edges, never stepping onto a quest of the other faction (quest_details.faction or a
.imperial./.republic. folder), and never using an ELIGIBLE gate (a sibling offer, not
an order). A leg with no path is a BREAK; the report then lists what the leg's start
does lead to, so the gap can be read.
"""
import collections
import os
import sys

out = sys.argv[1]
# Edge kinds left out (env EXCLUDE, comma list); default leaves out the loose 'touch'.
EXCLUDE = set(os.environ.get('EXCLUDE', 'touch').split(','))
TAG = os.environ.get('TAG', '')
name, fac = {}, {}
for line in list(open(f'{out}/quests.tsv'))[1:]:
    f, n, m, fa = line.rstrip('\n').split('\t')
    name[f], fac[f] = n, fa
succ = collections.defaultdict(dict)
for line in list(open(f'{out}/edges.tsv'))[1:]:
    a, b, k, via = line.rstrip('\n').split('\t')
    if k in EXCLUDE or (k == 'requires' and 'ELIGIBLE' in via):
        continue
    if b in succ[a]:
        k0, v0 = succ[a][b]
        succ[a][b] = ('+'.join(sorted(set(k0.split('+')) | {k})), v0 if k0 == 'requires' else via)
    else:
        succ[a][b] = (k, via)


def side(q):
    f = fac.get(q) or ''
    if f == 'empire' or '.imperial.' in q or '.empire.' in q:
        return 'imp'
    if f == 'republic' or '.republic.' in q:
        return 'rep'
    return ''


def leg(a, b, faction):
    par = {a: None}
    dq = collections.deque([a])
    while dq:
        x = dq.popleft()
        if x == b:
            p = []
            while x is not None:
                p.append(x)
                x = par[x]
            return p[::-1]
        for y in succ.get(x, {}):
            if y not in par and side(y) in ('', faction):
                par[y] = x
                dq.append(y)
    return None


def spine(f):
    """Waypoints for faction f ('imp' or 'rep')."""
    i = f == 'imp'
    F = 'imperial' if i else 'republic'
    return [
        'qst.location.ilum.world_arc.imperial.firststrike' if i else
        'qst.location.ilum.world_arc.republic.crystalball',
        'qst.location.flashpoint.shared.ilum.an_empire_betrayed' if i else
        'qst.location.flashpoint.shared.ilum.ilum_under_siege',
        'qst.exp.01.makeb.world_arc.imperial.hub_1.countdown_to_doomsday' if i else
        'qst.exp.01.makeb.world_arc.republic.hub_1.trouble_in_paradise',
        'qst.exp.01.makeb.world_arc.imperial.hub_2.revenge_of_the_archon' if i else
        'qst.exp.01.makeb.world_arc.republic.hub_2.taking_on_toborro',
        f'qst.exp.02.forged_alliances.{F}.forged_alliances',
        f'qst.exp.02.rishi.world_arc.{F}.hub_1.rishi_breadcrumb',
        f'qst.exp.02.yavin_4.world_arc.{F}.the_enemy_within',
        'qst.daily_area.ziost.shared.a_presence_on_ziost',
        'qst.daily_area.ziost.shared.the_end_of_it_all',
        'qst.exp.generic.breadcrumbs.breadcrumb_kotfe',
        'qst.exp.seasons.01.ep_01_the_hunt',
        'qst.exp.seasons.01.ep_16',
        'qst.exp.seasons.02.ep_01',
        'qst.exp.seasons.02.ep_09',
        'qst.daily_area.iokath.world.mission_1',
        f'qst.daily_area.iokath.world.mission_4_{i and "imp" or "rep"}',
        'qst.flashpoint.umbara.umbara_access_solo_wrapper',
        'qst.flashpoint.copero.copero_story_wrapper',
        'qst.flashpoint.nathema.nathema_story_wrapper',
        'qst.alliance.patriot_traitor.breadcrumb',
        'qst.alliance.patriot_traitor.ossus_01',
        'qst.daily_area.ossus.imperial.crit_path.mission_4' if i else
        'qst.daily_area.ossus.republic.crit_path.mission_4',
        'qst.alliance.allied_opinions.allied_opinions',
        f'qst.exp.03.intro.{F}.intro',
        f'qst.exp.03.mek_sha.world_arc.{F}.quest_4',
        'qst.flashpoint.corellia.corellia_story_wrapper_imp' if i else
        'qst.flashpoint.corellia.corellia_story_wrapper',
        'qst.exp.03.epilogue.shared.kira_scourge',
        f'qst.alliance.pinnacle_of_power.{F}.{i and "imp" or "rep"}_crit_path',
        'qst.alliance.pinnacle_of_power.shared.kira_scourge',
        'qst.alliance.camp.jekiah_intro.jekiah_intro',
        'qst.alliance.camp.jekiah_intro.jekiah_update',
        'qst.alliance.satele_ship.shared.tenebrae_final_weapon',
        'qst.flashpoint.spirit_of_vengeance.spirit_of_vengeance_wrapper',
        'qst.alliance.conflicting_priorities.conflicting_priorities',
        'qst.flashpoint.secrets_of_the_enclave.secrets_of_the_enclave_wrapper' if i else
        'qst.flashpoint.secrets_of_the_enclave.secrets_of_the_enclave_wrapper_rep',
        'qst.alliance.relentless_ambitions.relentless_ambitions',
        'qst.alliance.whispers_in_the_force.whispers_in_the_force',
        f'qst.exp.04.manaan.world_arc.{F}.mission_1',
        f'qst.exp.04.manaan.world_arc.{F}.mission_4',
        f'qst.flashpoint.elom.elom_wrapper_{i and "imp" or "rep"}',
        f'qst.exp.04.elom_digsite.digging_deeper_{i and "imp" or "rep"}',
        'qst.daily_area.ruhnuk.story.shared.mission_1',
        'qst.daily_area.ruhnuk.story.shared.mission_4',
        'qst.alliance.old_wounds.old_wounds',
        'qst.alliance.desperate_defiance.shared.intro',
        'qst.alliance.desperate_defiance.shared.quest_4',
        'qst.alliance.galactic_threads.odessen_intro',
        'qst.alliance.galactic_threads.ilum',
        'qst.alliance.masters_enigma.masters_enigma',
        f'qst.alliance.legacy_reborn.intro_{i and "imp" or "rep"}',
        'qst.alliance.legacy_reborn.khar_shian',
    ]


FINALES = [
    ('imp', 'bounty_hunter', 'qst.location.open_world.imperial.act_3.bounty_hunter.number_one_with_a_bullet'),
    ('imp', 'sith_inquisitor', 'qst.location.open_world.imperial.act_3.sith_inquisitor.the_dark_council'),
    ('imp', 'sith_warrior', 'qst.location.open_world.imperial.act_3.sith_warrior.retribution'),
    ('imp', 'imperial_agent', 'qst.location.open_world.imperial.act_3.spy.the_star_chamber'),
    ('rep', 'jedi_consular', 'qst.location.open_world.republic.act_3.jedi_consular.chaos_and_harmony'),
    ('rep', 'jedi_knight', 'qst.location.open_world.republic.act_3.jedi_knight.doomsday'),
    ('rep', 'smuggler', 'qst.location.open_world.republic.act_3.smuggler.finale_thefinalscore'),
    ('rep', 'trooper', 'qst.location.open_world.republic.act_3.trooper.the_battle_of_corellia'),
]


def lbl(q):
    return f'{name.get(q) or "?"} [{q}]'


with open(f'{out}/legs{TAG}.txt', 'w') as w:
    for f in ('imp', 'rep'):
        w.write(f'##### {f}\n')
        for fc, cls, fin in FINALES:
            if fc != f:
                continue
            p = leg(fin, spine(f)[0], f)
            w.write(f'{cls}: {lbl(fin)} -> {name[spine(f)[0]]}: '
                    f'{"BREAK" if not p else " > ".join(succ[a][b][0] for a, b in zip(p, p[1:]))}\n')
        wp = spine(f)
        for a, b in zip(wp, wp[1:]):
            p = leg(a, b, f)
            if p:
                kinds = [succ[x][y][0] for x, y in zip(p, p[1:])]
                w.write(f'ok    {name.get(a)} -> {name.get(b)}  ({len(p) - 1} hops: {",".join(kinds)})\n')
                if len(p) > 2:
                    for x, y in zip(p, p[1:]):
                        w.write(f'        {succ[x][y][0]:8s} {lbl(y)}  via {succ[x][y][1][:90]}\n')
            else:
                w.write(f'BREAK {lbl(a)} -/-> {lbl(b)}\n')
                for y, (k, via) in sorted(succ.get(a, {}).items()):
                    if side(y) in ('', f):
                        w.write(f'        leads to {k:8s} {lbl(y)}  via {via[:90]}\n')
print('wrote legs.txt')
