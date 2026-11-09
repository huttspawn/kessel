"""Resolve string ids (id2 << 32 | id1) against dumped STBs.
usage: stbjoin.py <stb dump>... ; reads ids from the taxi and dec dumps and prints coverage."""
import os
import json, sys
D = os.environ["KESSEL_SCRATCH"] + "/dec-economy"  # dumps from dump_kind_jsonl, dump_singleton, dump_stb
txt = {}
for f in sys.argv[1:]:
    for l in open(f):
        p = l.rstrip("\n").split("|", 3)
        if len(p) == 4:
            txt[(int(p[2]) << 32) | int(p[1])] = p[3]
tax = [json.loads(l) for l in open(D + "/kind/tax.jsonl")]
ok = [(t["fqn"], txt.get(t["v"].get("f75aa580"))) for t in tax if t["v"].get("f75aa580") is not None]
print("tax names", len(ok), "resolve", sum(1 for _, n in ok if n))
for f, n in ok[:8]:
    print("  ", f, "->", n)
dec = [json.loads(l) for l in open(D + "/kind/dec.jsonl")]
ok = [(t["fqn"], txt.get(t["v"].get("76712809"))) for t in dec]
print("dec names", len(ok), "resolve", sum(1 for _, n in ok if n))
for f, n in ok[:5]:
    print("  ", f, "->", n)
