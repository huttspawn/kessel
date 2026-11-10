"""Inside each 'Space {945872057663605}' interval of the combat logs: how long it lasts and which
abilities (with their ids) are activated before the next AreaEntered.

usage: python3 -I log_space_interval.py <log dir> [<log dir> ...]
"""
import os, re, sys, collections

AREA = re.compile(r'AreaEntered \{\d+\}: ([^{\]]*?) \{(\d+)\}')
TS = re.compile(r'^\[(\d+):(\d+):(\d+)\.(\d+)\]')
ABL = re.compile(r'\] \[([^\]\[{]+?) \{(\d+)\}\] \[Event \{\d+\}: AbilityActivate ')
SPACE = '945872057663605'
inside = collections.Counter(); rows = []


def sec(l):
    m = TS.match(l)
    return int(m.group(1)) * 3600 + int(m.group(2)) * 60 + int(m.group(3)) if m else None


for d in sys.argv[1:]:
    for f in sorted(os.listdir(d)):
        if not f.endswith('.txt'):
            continue
        lines = open(os.path.join(d, f), encoding='utf-8', errors='replace').read().splitlines()
        idx = [(i, m.group(2), m.group(1).strip()) for i, l in enumerate(lines) for m in [AREA.search(l)] if m]
        for j, (i, aid, name) in enumerate(idx):
            if aid != SPACE:
                continue
            end = idx[j + 1][0] if j + 1 < len(idx) else len(lines)
            abl = collections.Counter(f'{m.group(1).strip()} {{{m.group(2)}}}' for l in lines[i:end] for m in [ABL.search(l)] if m)
            for a in abl:
                inside[a] += 1
            t0, t1 = sec(lines[i]), sec(lines[end - 1])
            dur = (t1 - t0) % 86400 if t0 is not None and t1 is not None else None
            rows.append((f, lines[i][1:13], dur, end - i, abl.most_common(5)))
for r in rows:
    print(r)
print('abilities seen inside Space intervals (by interval count):', inside.most_common(25))


# Login signature: how many Space entries are followed within 5 lines by Safe Login, and at which position.
POS = re.compile(r'\|\(([-\d.]+),([-\d.]+),([-\d.]+),')
sig = collections.Counter(); pos = collections.Counter()
for d in sys.argv[1:]:
    for f in sorted(os.listdir(d)):
        if not f.endswith('.txt'):
            continue
        lines = open(os.path.join(d, f), encoding='utf-8', errors='replace').read().splitlines()
        for i, l in enumerate(lines):
            if 'AreaEntered' in l and SPACE in l:
                sig['safe login within 5 lines' if any('Safe Login' in x for x in lines[i:i + 6]) else 'no safe login'] += 1
                m = POS.search(l); pos[m.groups() if m else None] += 1
        # how often does a log's first AreaEntered come with Safe Login in general (control)
print('Space entries:', dict(sig)); print('positions:', pos.most_common(5))
ctl = collections.Counter()
for d in sys.argv[1:]:
    for f in sorted(os.listdir(d)):
        if not f.endswith('.txt'):
            continue
        lines = open(os.path.join(d, f), encoding='utf-8', errors='replace').read().splitlines()
        for i, l in enumerate(lines):
            if 'AreaEntered' in l and SPACE not in l:
                ctl['other area, safe login within 5' if any('Safe Login' in x for x in lines[i:i + 6]) else 'other area, none'] += 1
                if i + 1 < len(lines) and 'DisciplineChanged' in lines[i + 1]:
                    ctl['other area, DisciplineChanged next'] += 1
print('control:', dict(ctl))
