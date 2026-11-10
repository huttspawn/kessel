"""Ability flag 4000000197d6d791 against what the ability's own tooltip says about stealth.
Classes: 'requires stealth' (only from stealth), 'usable in stealth / does not break stealth',
'breaks stealth' mentions, other. Also which other ability fields predict 'requires stealth'.
Canonical abilities only.
usage: stealth_text.py <abl_all.jsonl> <spice.sqlite>"""
import json, re, sys, sqlite3, collections
abl, db = sys.argv[1:3]
con = sqlite3.connect(db)
desc = {}
for id2, text in con.execute("select id2, text from strings where fqn like 'str.abl.1.%'"):
    desc[int(id2)] = text or ''
sid = {f: int(s) for f, s in con.execute("select fqn, string_id from objects where kind='Ability' and is_canonical=1 and string_id is not null")}
canon = {g for (g,) in con.execute("select guid from objects where kind='Ability' and is_canonical=1")}
FLAG = '4000000197d6d791'
cls = collections.Counter(); ex = collections.defaultdict(list); fields = collections.defaultdict(collections.Counter); ntot = collections.Counter()
REQ = re.compile(r'requires stealth|only (be )?usable (while|in|from) stealth|can only be used (while|in|from) stealth|must be (in )?stealth', re.I)
USE = re.compile(r'(does not|doesn.t|will not|won.t) break stealth|(usable|can be used|may be used) (while|in|from) stealth', re.I)
for line in open(abl):
    o = json.loads(line)
    f = o['fqn']
    if f not in sid or o.get('guid') not in canon:
        continue
    t = desc.get(sid[f], '')
    if not t:
        continue
    flag = bool(o['v'].get(FLAG))
    if REQ.search(t):
        k = 'requires stealth'
    elif USE.search(t):
        k = 'usable in stealth / does not break'
    elif re.search(r'stealth', t, re.I):
        k = 'other stealth mention'
    else:
        k = 'no stealth mention'
    cls[(k, flag)] += 1
    if k != 'no stealth mention' and len(ex[(k, flag)]) < 6:
        m = re.search(r'.{0,50}stealth.{0,40}', t, re.I)
        ex[(k, flag)].append((f, m.group(0) if m else ''))
    for fid in o['v']:
        fields[k][fid] += 1
    ntot[k] += 1
for k in sorted(cls):
    print(cls[k], k)
for k, v in ex.items():
    print(k)
    for x in v:
        print('    ', x)
print('fields most specific to requires-stealth:')
base = ntot['no stealth mention']
for fid, n in fields['requires stealth'].most_common(60):
    share = n / ntot['requires stealth']; bshare = fields['no stealth mention'][fid] / base
    if share > 0.6 and bshare < 0.1:
        print('   ', fid, round(share, 2), round(bshare, 3))
