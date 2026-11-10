"""Every string value in hyd.jsonl: tokens (words_hyd.txt) and whole strings, last fqn
segments and ':'-tails (names_hyd.txt), uppercased. usage: hydwords.py <hyd.jsonl> <out_dir>"""
import json, re, sys, os, collections
tok = collections.Counter(); whole = set()


def walk(x):
    if isinstance(x, dict):
        for k, v in x.items():
            walk(k); walk(v)
    elif isinstance(x, list):
        for v in x:
            walk(v)
    elif isinstance(x, str) and len(x) < 120:
        u = x.upper()
        whole.update({u, u.rsplit('.', 1)[-1], u.split(':', 1)[-1]})
        for w in re.split(r'[^A-Z0-9]+', u):
            if w:
                tok[w] += 1


for line in open(sys.argv[1]):
    walk(json.loads(line))
out = sys.argv[2]
open(os.path.join(out, 'words_hyd.txt'), 'w').write('\n'.join(w for w, _ in tok.most_common() if len(w) <= 20) + '\n')
open(os.path.join(out, 'names_hyd.txt'), 'w').write('\n'.join(sorted(w for w in whole if w)) + '\n')
print(len(tok), len(whole))
