"""Fields (and scalar values) shared by the 4 unflagged tech openers that break stealth (Backstab,
Back Blast, Lethal Strike, Point Blank Shot) and rare on other unflagged player abilities.
usage: opener_fields.py <abl_all.jsonl> <spice.sqlite>"""
import json, sys, sqlite3, collections
abl, db = sys.argv[1:3]
OPEN = {'abl.agent.backstab', 'abl.smuggler.back_blast', 'abl.agent.skill.lethality.lethal_strike', 'abl.smuggler.skill.ruffian.point_blank_shot'}
con = sqlite3.connect(db)
canon = {g for (g,) in con.execute("select guid from objects where kind='Ability' and is_canonical=1")}
fo = collections.Counter(); fu = collections.Counter(); nu = 0; seen = 0
for line in open(abl):
    o = json.loads(line)
    if o.get('guid') not in canon:
        continue
    f = o['fqn']; v = o['v']
    if not (f.split('.')[1] in ('agent', 'smuggler', 'bounty_hunter', 'trooper', 'sith_warrior', 'jedi_knight', 'sith_inquisitor', 'jedi_consular')):
        continue
    feats = set()
    for k, x in v.items():
        feats.add(k[-8:])
        if isinstance(x, (bool, int, float, str)) and len(str(x)) < 24:
            feats.add('%s=%s' % (k[-8:], x))
        if isinstance(x, dict) and 'enum' in x:
            feats.add('%s=e%s' % (k[-8:], x['enum']))
    if f in OPEN:
        seen += 1; fo.update(feats)
    elif not v.get('4000000197d6d791') and not v.get('400000003d2e41ef'):
        nu += 1; fu.update(feats)
print('openers found', seen, 'other unflagged actives', nu)
for k, n in fo.items():
    if n == seen and fu[k] / max(nu, 1) < 0.15:
        print('  ', k, 'openers', n, 'other unflagged share %.3f' % (fu[k] / nu))
