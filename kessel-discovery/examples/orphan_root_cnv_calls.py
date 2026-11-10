"""Decode-level check of the conversation callers found by orphan_scan2: a node script call
(2bd6139d list entry) whose 2bd61399 names the root script and whose 2bd6139a is the handler's event.
usage: verify_cnv_calls.py <cnv_actions binary> <spice.sqlite> <orphan_scan2.tsv> <out.tsv>"""
import sys, json, subprocess, collections
binp, db, scan, out = sys.argv[1:5]
need = collections.defaultdict(set)
rows = []
for l in open(scan):
    s, h, codes, hits, achs = l.rstrip('\n').split('\t')
    for kind, fqn, canon, how in json.loads(hits):
        if kind == 'Conversation':
            need[fqn].add((s, h[len('On Custom:'):]))
    rows.append((s, h, codes, json.loads(hits), achs))
proved = {}
for cnv, pairs in need.items():
    p = subprocess.run([binp, db, '--all', cnv], capture_output=True, text=True)
    for line in p.stdout.splitlines():
        o = json.loads(line)
        calls = []
        stack = [o.get('f') or {}]
        while stack:
            x = stack.pop()
            if isinstance(x, dict):
                if '2bd61399' in x:
                    calls.append((x.get('2bd61399'), x.get('2bd6139a')))
                stack.extend(x.values())
            elif isinstance(x, list):
                stack.extend(x)
        for hy, ev in calls:
            for s, name in pairs:
                if isinstance(hy, str) and hy.endswith(':' + s) and ev == name:
                    proved.setdefault((s, name), []).append((cnv, o.get('node')))
c = collections.Counter()
with open(out, 'w') as o:
    for s, h, codes, hits, achs in rows:
        name = h[len('On Custom:'):]
        kinds = {x[0] for x in hits}
        if (s, name) in proved:
            st = 'conversation node call (decoded)'
        elif 'Dynamic' in kinds:
            st = 'dynamic object (byte match)'
        elif hits:
            st = 'byte match only'
        else:
            st = 'no caller in client'
        c[st] += 1
        o.write('%s\t%s\t%s\t%s\t%s\t%s\n' % (s, h, codes, st, json.dumps(proved.get((s, name), [])[:4]), achs))
print(dict(c))
