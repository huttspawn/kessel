"""Count every key of lgcAchievementEventsPrototype once, by what raises it (#368 criterion 4).

usage: python3 -I achievement_event_raisers.py <lgcach.json> <kindfqn.tsv> <hidden_earnables.tsv>
       <hook_callers.tsv> <achievement_event_keys output (varkeys)> [strings.txt ...] > table.txt

lgcach.json: dump_singleton of lgcAchievementEventsPrototype (field 55f64de0 = code -> achievements).
kindfqn.tsv: `select kind||char(9)||fqn from objects` on an unfiltered extract.
hidden_earnables.tsv: hidden_earnables.py output (every Hydra 'Achievement Event' action with its
  root triggers; a root marked '(no caller found)' is an orphan On Custom handler).
hook_callers.tsv: the effects group's table of orphan roots called by an ability effect
  (effAction_ExecuteHydraHook); caller_match 'exact' means the effect names that hook.
varkeys: achievement_event_keys.py output over the unfiltered Quest/Hydra/... dumps; a line holds
  a non-E000 key and where it occurs as a value (field 2c1fe390 = qstVariableGUID,
  e4557d03 = mapLinkAreaId).

Each key lands in exactly one row. Precedence: object fqn > script-raised name, traced > orphan.
"""
import json, sys, collections

M = (1 << 64) - 1


def fnv(s):
    h = 0xcbf29ce484222325
    for b in s.encode('latin-1', 'replace'):
        h ^= b; h = (h * 0x100000001b3) & M
    return h


def fold(n):
    h = fnv(n.upper())
    return 0xE000000000000000 | (h & 0x0000FFFFFFFF0000) | ((h ^ (h >> 48)) & 0xFFFF)


lgc, kindfqn, hidden, callers, varkeys = sys.argv[1:6]
keys = [e['k'] & M for e in json.load(open(lgc))['55f64de0'] if isinstance(e['k'], int)]
assert len(keys) == len(set(keys))

KIND = {'Npc': 'NPC kill', 'Achievement': 'achievement earned', 'Quest': 'quest completed',
        'Item': 'item', 'Codex': 'codex', 'Ability': 'ability learned (abl fqn)',
        'NpcCompanion': 'companion'}
by_fqn = {}
for line in open(kindfqn):
    k, f = line.rstrip('\n').split('\t', 1)
    lab = KIND.get(k) or ('operation completed (rdd fqn)' if f.startswith('rdd.') else None)
    if lab is None:
        lab = f'other object ({k})'
    by_fqn.setdefault(fold(f), lab)

# script-raised names: traced if any raising action has a root that is not an orphan, or an
# orphan root an ability effect calls (hook_callers exact).
exact = set()
hdr = None
for line in open(callers):
    p = line.rstrip('\n').split('\t')
    if hdr is None:
        hdr = p; continue
    r = dict(zip(hdr, p))
    if r['caller_match'] == 'exact':
        exact.add((r['root_script'], r['root_handler']))
script = {}   # code -> 'traced' / 'orphan'
trig = collections.defaultdict(collections.Counter)
hdr = None
for line in open(hidden):
    p = line.rstrip('\n').split('\t')
    if hdr is None:
        hdr = p; continue
    r = dict(zip(hdr, p))
    if not r['event_code']:
        continue
    c = int(r['event_code'], 16)
    traced = False
    for root in r['root_triggers'].split(' | '):
        if root.endswith('(no caller found)'):
            s, h = root[:-len(' (no caller found)')].split(' ', 1)
            if (s, h) in exact:
                traced = True; trig[c]['effect hook'] += 1
        else:
            traced = True
            trig[c][root.split(' ', 1)[1].split(':')[0] if ' ' in root else root] += 1
    if traced or script.get(c) != 'traced':
        script[c] = 'traced' if traced else script.get(c, 'orphan')

var = {}
for line in open(varkeys):
    if not line.startswith('0x'):
        continue
    k = int(line.split()[0], 16)
    if "'2c1fe390'" in line:
        var[k] = 'quest variable (qstVariableGUID)'
    elif "'e4557d03'" in line:
        var[k] = 'area id (mapLinkAreaId)'
    elif line.rstrip().endswith('[]'):
        var[k] = 'variable-shaped, no holder'

# optional: every string value in the dumps (vocab.py output); a code that folds from one of
# them but from no object fqn or raising action has a name and nothing that raises it.
text = {}
keyset = set(keys)
for p in sys.argv[6:]:
    for line in open(p, errors='replace'):
        s = line.rstrip('\n')
        toks = {s, *s.replace(':', ' ').replace('"', ' ').split(' ')}
        parts = s.lstrip('@').split('.')
        toks |= {'.'.join(parts[:i]) for i in range(2, len(parts))}  # fqn prefixes
        for tok in toks:
            tok = tok.strip().lstrip('@')
            c = fold(tok)
            if tok and c in keyset and c not in by_fqn and c not in script:
                text.setdefault(c, tok)

rows = collections.Counter()
overlap = collections.Counter()
for k in keys:
    if k >> 48 == 0xE000:
        if k in by_fqn:
            rows[('a', by_fqn[k])] += 1
            if k in script:
                overlap[by_fqn[k]] += 1
        elif script.get(k) == 'traced':
            rows[('a', 'script-raised, caller traced')] += 1
        elif script.get(k) == 'orphan':
            rows[('b', 'script-raised, only under orphan On Custom roots')] += 1
        elif k in text:
            rows[('b', 'name held only as text (no object, no raiser)')] += 1
            print('text-named', f'{k:016X}', text[k], file=sys.stderr)
        else:
            rows[('c', 'E000 code, no name in the client')] += 1
    elif k < 1 << 32:
        rows[('d', 'engine stat counter (small integer)')] += 1
    else:
        lab = var.get(k, 'non-E000, unclassified')
        rows[('c' if 'no holder' in lab or 'unclassified' in lab else 'a', lab)] += 1
total = sum(rows.values())
print('keys', len(keys), 'counted', total)
for (g, lab), n in sorted(rows.items(), key=lambda x: (x[0][0], -x[1])):
    print(f'{g}\t{lab}\t{n}')
for g in 'abcd':
    print(f'total {g}', sum(n for (gg, _), n in rows.items() if gg == g))
print('fqn-named codes also raised by a script:', dict(overlap))
roots = collections.Counter()
for c, s in script.items():
    if s == 'traced' and c not in by_fqn and c in set(keys):
        roots[trig[c].most_common(1)[0][0]] += 1
print('traced script-raised events by main trigger:', roots.most_common(15))
