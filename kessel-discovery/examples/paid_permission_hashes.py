"""Structured variants of a permission row name, hashed FNV-1a 64 over UPPER, against that row's ids."""
import json, sys, itertools
D = sys.argv[1]
d = json.load(open(D + '/chrPaidPermissionDefsTablePrototype.json'))

def f64(s):
    h = 0xcbf29ce484222325
    for b in s.encode():
        h ^= b
        h = (h * 0x100000001b3) & 0xffffffffffffffff
    return h

pre = ['', 'PAID_PERMISSION.', 'PAID_PERMISSION_', 'ABL.', 'STAT_', 'STAT.', 'EFF.', 'TWEAK.', 'MOD.', 'PP.', 'PAIDPERMISSION', 'PERMISSION.', 'CHR', 'CHRPAIDPERMISSION', 'BOOST.', 'ITM.']
suf = ['', '.ABILITY', '_ABILITY', '.EFFECT', '_EFFECT', '.STAT', '_STAT', '.MODIFIER', '_MODIFIER', '.BUFF', '_BUFF', '.DEBUFF', '.MULT', '_MULT', '_MULTIPLIER', '.MULTIPLIER', '.VALUE', '_VALUE', '.RATE', '.AMOUNT', '.MAX', '.MIN', '.ID', '_ID', '.F2P', '.SUB', '.PREFERRED', '.FREE', '.SUBSCRIBER', '_F2P', '_SUB', '_PREFERRED', '_FREE', '_SUBSCRIBER']
hits = 0
for r in d['9da4d447']:
    ids = {}
    for k in ('c28dfca0', 'b6a43670'):
        if r.get(k) is not None:
            ids[r[k] & 0xffffffffffffffff] = k
    if not ids:
        continue
    n = r['c3f22098'].upper()
    core = n.replace('PAID_PERMISSION.', '')
    forms = {n, core, core.replace('.', '_'), n.replace('.', '_'), core.split('.')[-1], core.replace('.', '')}
    for p, f, s in itertools.product(pre, forms, suf):
        c = p + f + s
        h = f64(c)
        if h in ids:
            hits += 1
            print('HIT', r['c3f22098'], ids[h], c)
print('hits', hits)
