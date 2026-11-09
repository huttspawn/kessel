"""Fold candidate strings to E000 event codes / GUIDs and match the unresolved list.

usage: fold.py unresolved.json vocab1 [vocab2 ...]
code = 0xE000<<48 | (h & 0x0000FFFFFFFF0000) | ((h ^ (h>>48)) & 0xFFFF), h = fnv1a64(upper(s)).
Candidates: the line, every '.'/' '/':' token, the part after '@', and the suffix after each '.'.
"""
import json, sys

P = 0x100000001b3; M = (1 << 64) - 1; OFF = 0xcbf29ce484222325


def fold(s):
    h = OFF
    for c in s.encode('latin-1', 'replace'):
        h ^= c; h = (h * P) & M
    return 0xE000000000000000 | (h & 0x0000FFFFFFFF0000) | ((h ^ (h >> 48)) & 0xFFFF)


want = {int(x, 16) for x in json.load(open(sys.argv[1]))}
print('targets', len(want), file=sys.stderr)
hits = {}
seen = set()
for p in sys.argv[2:]:
    for line in open(p, errors='replace'):
        s = line.rstrip('\n').lstrip('@')
        cs = {s}
        for sep in (' ', ':', '\t', '/', '"'):
            cs.update(s.split(sep))
        parts = s.split('.')
        for i in range(1, len(parts)):
            cs.add('.'.join(parts[i:]))
            cs.add('.'.join(parts[:i]))
        for c in cs:
            c = c.strip().upper()
            if not c or c in seen:
                continue
            seen.add(c)
            f = fold(c)
            if f in want:
                hits.setdefault(f, set()).add(c)
print('candidates', len(seen), 'hits', len(hits), file=sys.stderr)
for f, n in sorted(hits.items()):
    print(f'{f:016X}', sorted(n)[:5], sep='\t')
