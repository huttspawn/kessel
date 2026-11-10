"""The orphan On Custom roots behind script-raised achievement events that no effect hook calls,
and the events reached only through them (push-1 raiser table row b, 211 events / 457 roots).
usage: orphans.py <hidden_earnables.tsv> <hook_callers.tsv> <lgcach.json> <out.tsv>
out: root_script, root_handler, event codes (comma), achievements (space)"""
import sys, json, collections
hidden, callers, lgc, out = sys.argv[1:5]
M = (1 << 64) - 1
keys = {e['k'] & M for e in json.load(open(lgc))['55f64de0'] if isinstance(e['k'], int)}
exact = set(); hdr = None
for line in open(callers):
    p = line.rstrip('\n').split('\t')
    if hdr is None:
        hdr = p; continue
    r = dict(zip(hdr, p))
    if r['caller_match'] == 'exact':
        exact.add((r['root_script'], r['root_handler']))
traced = set(); roots = collections.defaultdict(lambda: [set(), set()]); code_roots = collections.defaultdict(set)
hdr = None
for line in open(hidden):
    p = line.rstrip('\n').split('\t')
    if hdr is None:
        hdr = p; continue
    r = dict(zip(hdr, p))
    if not r['event_code']:
        continue
    c = int(r['event_code'], 16)
    for root in r['root_triggers'].split(' | '):
        if root.endswith('(no caller found)'):
            s, h = root[:-len(' (no caller found)')].split(' ', 1)
            if (s, h) in exact:
                traced.add(c)
            else:
                roots[(s, h)][0].add(c); roots[(s, h)][1].update(r['achievements'].split())
                code_roots[c].add((s, h))
        else:
            traced.add(c)
orph_codes = {c for c in code_roots if c not in traced and c in keys}
# keep roots that reach at least one orphan-only code
keep = {k: v for k, v in roots.items() if v[0] & orph_codes}
with open(out, 'w') as o:
    for (s, h), (cs, achs) in sorted(keep.items()):
        o.write('%s\t%s\t%s\t%s\n' % (s, h, ','.join('%016X' % c for c in sorted(cs & orph_codes)), ' '.join(sorted(achs))))
print('orphan-only codes', len(orph_codes), 'roots', len(keep), 'all orphan roots', len(roots))
