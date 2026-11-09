"""Walk the story spine built by story_spine.py.

usage: python3 -I story_spine_walk.py <outdir> [kinds]
  kinds: comma list of edge kinds to use (default requires,handoff,script)
Reads edges.tsv, quests.tsv; writes walk_<kinds>.txt and families_<kinds>.tsv.

Family table: arc quests = named quests under the family prefixes (utility, globals,
var holders and crawls dropped); largest chain = largest weakly connected component
of the family's arc quests using edges between them; isolated = arc quests with no
such edge.
"""
import collections
import os
import sys

out = sys.argv[1]
kinds = set((sys.argv[2] if len(sys.argv) > 2 else 'requires,handoff,script').split(','))
tag = '_'.join(sorted(k[:3] for k in kinds)) + os.environ.get('TAG', '')

name, mtype = {}, {}
for line in list(open(f'{out}/quests.tsv'))[1:]:
    f, n, m, _ = line.rstrip('\n').split('\t')
    name[f], mtype[f] = n, m
succ = collections.defaultdict(dict)
for line in list(open(f'{out}/edges.tsv'))[1:]:
    a, b, k, via = line.rstrip('\n').split('\t')
    if k in kinds:
        succ[a].setdefault(b, (k, via))

FAMILIES = [
    ('makeb', ['qst.exp.01.makeb.']),
    ('ziost', ['qst.exp.01.ziost.']),
    ('ziost_story', ['qst.daily_area.ziost.shared.', 'qst.daily_area.ziost.breadcrumbs.']),
    ('forged_alliances', ['qst.exp.02.forged_alliances.']),
    ('rishi', ['qst.exp.02.rishi.']),
    ('yavin_4', ['qst.exp.02.yavin_4.']),
    ('kotfe', ['qst.exp.seasons.01.']),
    ('kotet', ['qst.exp.seasons.02.']),
    ('iokath', ['qst.daily_area.iokath.world.']),
    ('umbara', ['qst.flashpoint.umbara.']),
    ('copero', ['qst.flashpoint.copero.']),
    ('ossus', ['qst.alliance.patriot_traitor.', 'qst.daily_area.ossus.imperial.crit_path',
               'qst.daily_area.ossus.republic.crit_path']),
    ('nathema', ['qst.flashpoint.nathema.']),
    ('meridian', ['qst.flashpoint.corellia.']),
    ('onslaught_intro', ['qst.exp.03.intro.']),
    ('onderon', ['qst.exp.03.onderon.']),
    ('mek_sha', ['qst.exp.03.mek_sha.']),
    ('spirit_of_vengeance', ['qst.flashpoint.spirit_of_vengeance.']),
    ('secrets_of_the_enclave', ['qst.flashpoint.secrets_of_the_enclave.']),
    ('dantooine', ['qst.dynamic_events.dantooine.missions.']),
    ('manaan', ['qst.exp.04.manaan.', 'qst.exp.04.empire.', 'qst.exp.04.republic.']),
    ('elom', ['qst.flashpoint.elom.', 'qst.exp.04.elom_digsite.']),
    ('galactic_threads', ['qst.alliance.galactic_threads.']),
    ('legacy_reborn', ['qst.alliance.legacy_reborn.']),
    ('old_wounds', ['qst.alliance.old_wounds.']),
    ('masters_enigma', ['qst.alliance.masters_enigma.']),
    ('pinnacle_of_power', ['qst.alliance.pinnacle_of_power.']),
    ('ord_mantell', ['qst.daily_area.ord_mantell.story.']),
    ('desperate_defiance', ['qst.alliance.desperate_defiance.']),
    ('ruhnuk', ['qst.daily_area.ruhnuk.story.']),
    ('7x_singles', ['qst.alliance.relentless_ambitions.', 'qst.alliance.whispers_in_the_force.',
                    'qst.alliance.conflicting_priorities.', 'qst.alliance.allied_opinions.',
                    'qst.alliance.satele_ship.']),
]
DROP = ('utility', 'global', 'var_holder', 'crawl', 'logic', 'tracker')


def family(q):
    for fam, pres in FAMILIES:
        if any(q.startswith(p) for p in pres):
            return fam
    return None


ARC_TYPES = {'expansion', 'planetary_arc', 'class', 'alliance', 'flashpoint'}


ARC = os.environ.get('ARC', 'named')


def arc(q):
    if ARC == 'all':
        return True
    ok = bool(name.get(q)) and not any(d in q for d in DROP)
    return ok and (ARC == 'named' or mtype.get(q) in ARC_TYPES)


FINALES = {
    'bounty_hunter': 'qst.location.open_world.imperial.act_3.bounty_hunter.number_one_with_a_bullet',
    'sith_inquisitor': 'qst.location.open_world.imperial.act_3.sith_inquisitor.the_dark_council',
    'sith_warrior': 'qst.location.open_world.imperial.act_3.sith_warrior.retribution',
    'imperial_agent': 'qst.location.open_world.imperial.act_3.spy.the_star_chamber',
    'jedi_consular': 'qst.location.open_world.republic.act_3.jedi_consular.chaos_and_harmony',
    'jedi_knight': 'qst.location.open_world.republic.act_3.jedi_knight.doomsday',
    'smuggler': 'qst.location.open_world.republic.act_3.smuggler.finale_thefinalscore',
    'trooper': 'qst.location.open_world.republic.act_3.trooper.the_battle_of_corellia',
}


def lbl(q):
    return f'{name.get(q) or "?"} [{q}]'


def bfs(start):
    par, dist = {start: None}, {start: 0}
    dq = collections.deque([start])
    while dq:
        a = dq.popleft()
        for b in succ.get(a, {}):
            if b not in par:
                par[b], dist[b] = a, dist[a] + 1
                dq.append(b)
    return par, dist


def path(par, q):
    p = []
    while q is not None:
        p.append(q)
        q = par[q]
    return p[::-1]


fam_names = [f for f, _ in FAMILIES]
with open(f'{out}/walk_{tag}.txt', 'w') as w:
    for cls, fin in FINALES.items():
        start = fin.replace('.' + fin.split('.')[-1], '.hidden_act_3_completion')
        par, dist = bfs(fin)
        par2, _ = bfs(start)
        reached = {}
        for q in par:
            fam = family(q)
            if fam and arc(q) and (fam not in reached or dist[q] < dist[reached[fam]]):
                reached[fam] = q
        w.write(f'== {cls}: {lbl(fin)}  reaches {len(par)} quests\n')
        w.write('   families reached: ' + ', '.join(
            f'{f}@{dist[reached[f]]}' for f in fam_names if f in reached) + '\n')
        missing = [f for f in fam_names if f not in reached]
        w.write('   families NOT reached: ' + ', '.join(missing) + '\n')
        if start in par2:
            extra = [f for f in missing if any(family(q) == f and arc(q) for q in par2)]
            w.write(f'   (from {start.split(".")[-1]} also: {extra})\n')
        newest = [f for f in fam_names if f in reached][-1:] or []
        for f in newest:
            w.write(f'   path to newest family {f}:\n')
            for q in path(par, reached[f]):
                k = succ[par[q]][q][0] if par[q] else 'start'
                w.write(f'     {k:9s} {lbl(q)}\n')

    # Global connectivity of the families.
    w.write('\n== family -> family edges (count)\n')
    ff = collections.Counter()
    for a in succ:
        for b in succ[a]:
            fa, fb = family(a), family(b)
            if fa and fb and fa != fb and arc(a) and arc(b):
                ff[(fa, fb)] += 1
    for (fa, fb), n in sorted(ff.items(), key=lambda x: (fam_names.index(x[0][0]), -x[1])):
        w.write(f'   {fa} -> {fb}: {n}\n')

with open(f'{out}/families_{tag}.tsv', 'w') as t, open(f'{out}/isolated_{tag}.tsv', 'w') as isolated:
    t.write('family\tarc_quests\tlargest_chain\tpct\tisolated\tcomponents\ttop2_pct\n')
    for fam, _ in FAMILIES:
        qs = {q for q in name if family(q) == fam and arc(q)}
        adj = collections.defaultdict(set)
        for a in qs:
            for b in succ.get(a, {}):
                if b in qs:
                    adj[a].add(b)
                    adj[b].add(a)
        seen, comps = set(), []
        for q in qs:
            if q in seen:
                continue
            stack, comp = [q], set()
            while stack:
                x = stack.pop()
                if x in comp:
                    continue
                comp.add(x)
                stack.extend(adj[x] - comp)
            seen |= comp
            comps.append(comp)
        big = max((len(c) for c in comps), default=0)
        iso = sum(1 for q in qs if not adj[q])
        pct = round(100 * big / len(qs)) if qs else 0
        # Imperial and Republic arcs are separate components by design: the two largest.
        sizes = sorted((len(c) for c in comps), reverse=True)
        top2 = round(100 * sum(sizes[:2]) / len(qs)) if qs else 0
        t.write(f'{fam}\t{len(qs)}\t{big}\t{pct}%\t{iso}\t{len(comps)}\t{top2}%\n')
        for q in sorted(qs):
            if not adj[q]:
                isolated.write(f'{fam}\t{mtype.get(q)}\t{name.get(q)}\t{q}\n')
print('wrote', tag)
