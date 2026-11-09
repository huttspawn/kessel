"""Resolve cnqSchedulePrototype weeks to conquest events by cnqConquestInfoId; find a known run."""
import json, sys, sqlite3
D, db = sys.argv[1:3]
info = json.load(open(D + '/cnqConquestInfo.json'))
idname = {x['v'].get('8fbb75c1'): x['v'].get('8c7dafec') for x in info['295b3318']}
sch = json.load(open(D + '/cnqSchedulePrototype.json'))['8dcc7705']
weeks = [(e['8fbb75c0'], idname.get(e['8dcc7703'], 'UNRESOLVED:%d' % e['8dcc7703'])) for e in sch]
print('weeks', len(weeks), 'unresolved', sum(1 for w in weeks if w[1].startswith('UNRES')))
seq = [n for _, n in weeks]
run = sys.argv[3].split(',')
for i in range(len(seq) - len(run) + 1):
    if seq[i:i + len(run)] == run:
        print('run at week', weeks[i][0], seq[max(0, i - 3):i + len(run) + 3])
con = sqlite3.connect(db)
k = dict(con.execute('select week_ordinal, event_name from conquest_schedule'))
diff = sum(1 for w, n in weeks if k.get(w) != n)
print('kessel conquest_schedule.event_name differs from cnqConquestInfoId join on', diff, 'of', len(weeks))
print('first 6 kessel:', [k.get(w) for w, _ in weeks[:6]], 'join:', seq[:6])
import collections
print(collections.Counter(seq).most_common())
