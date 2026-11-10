"""Build uppercase word lists for fnv_words: big (every source) and core (frequent in
hydra events, gom names, field names, quest/hydra vocab).
usage: mkwords.py <hyd_events.txt> <vocab_gom.txt> <names.json> <vocab_fqn.txt> <out_dir>"""
import re, sys, json, collections, os
ev, gom, names, fqn, out = sys.argv[1:6]
cnt = collections.Counter(); core = collections.Counter()


def camel(s):
    return re.findall(r'[A-Z]?[a-z]+|[A-Z]+(?![a-z])|\d+', s)


for line in open(ev):
    e = line.strip().split(None, 1)
    if len(e) == 2:
        n = int(e[0]) if e[0].isdigit() else 1
        for w in re.split(r'[^A-Za-z0-9]+', e[1]):
            if w:
                core[w.upper()] += n; cnt[w.upper()] += n
for line in open(gom, errors='ignore'):
    for w in camel(line):
        core[w.upper()] += 1; cnt[w.upper()] += 1
for r in json.load(open(names))['records']:
    for w in camel(r['name']):
        core[w.upper()] += 1; cnt[w.upper()] += 1
for line in open(fqn, errors='ignore'):
    for w in re.split(r'[^A-Za-z0-9]+', line):
        if w:
            cnt[w.upper()] += 1
good = lambda w: 1 < len(w) <= 16 or w in ('A', 'I') or w.isdigit() and len(w) <= 2
big = [w for w, n in cnt.most_common() if good(w)][:60000]
c1 = [w for w, n in core.most_common() if good(w)][:6000]
c2 = [w for w, n in core.most_common() if good(w)][:1500]
for name, ws in (('big', big), ('core', c1), ('tiny', c2)):
    open(os.path.join(out, 'words_%s.txt' % name), 'w').write('\n'.join(ws) + '\n')
    print(name, len(ws))
