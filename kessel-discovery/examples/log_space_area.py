"""What surrounds the combat-log area 'Space {945872057663605}': the AreaEntered before and after it,
and the non-combat events (abilities used, conversations) in the 40 lines before it.

usage: python3 -I log_space_area.py <log dir> [<log dir> ...]

Reads every *.txt combat log (UTF-8 or latin-1). An AreaEntered line looks like
[time] [@Player#id|...] [] [] [AreaEntered {836045448953664}: Area Name {id} [Difficulty]] ...
"""
import os, re, sys, collections

AREA = re.compile(r'AreaEntered \{\d+\}: ([^{\]]*?) \{(\d+)\}')
ABL = re.compile(r'\] \[([^\]\[{]+?) \{(\d+)\}\] \[Event \{\d+\}: (AbilityActivate|AbilityDeactivate) ')
SPACE = '945872057663605'
before = collections.Counter(); after = collections.Counter(); prev_abl = collections.Counter()
n = 0; ex = []
for d in sys.argv[1:]:
    for f in sorted(os.listdir(d)):
        if not f.endswith('.txt'):
            continue
        try:
            lines = open(os.path.join(d, f), encoding='utf-8', errors='replace').read().splitlines()
        except OSError:
            continue
        areas = [(i, m.group(1).strip(), m.group(2)) for i, l in enumerate(lines) for m in [AREA.search(l)] if m]
        for j, (i, name, aid) in enumerate(areas):
            if aid != SPACE:
                continue
            n += 1
            p = areas[j - 1][1] if j > 0 else '(log start)'
            q = areas[j + 1][1] if j + 1 < len(areas) else '(log end)'
            before[p] += 1; after[q] += 1
            abls = [m.group(1).strip() for l in lines[max(0, i - 40):i] for m in [ABL.search(l)] if m]
            for a in set(abls):
                prev_abl[a] += 1
            if len(ex) < 12:
                ex.append((f, lines[i][:16], p, q, abls[-4:]))
print('Space AreaEntered lines:', n)
print('area before:', before.most_common(12))
print('area after :', after.most_common(12))
print('abilities in the 40 lines before:', prev_abl.most_common(15))
for e in ex:
    print('  ', e)
