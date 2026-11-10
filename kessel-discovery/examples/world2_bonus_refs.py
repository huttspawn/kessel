#!/usr/bin/env python3
"""Decode push 2, world: which payload field ties a bonus mission to its parent quest.

Walks every decoded qst payload (dump_kind_jsonl output) and records every
object reference (rendered "@fqn", or a bare integer GUID) to another object, with the field path.
Writes refs.tsv (src_fqn, path, target_fqn, target_kind) for later joins.

Usage: world2_bonus_refs.py <unfiltered.sqlite> <qst.jsonl> <out_dir>
"""
import json
import os
import sqlite3
import sys

db, qst, out = sys.argv[1], sys.argv[2], sys.argv[3]
con = sqlite3.connect(db)
g2 = {}
for guid, fqn, kind in con.execute("select guid, fqn, kind from objects"):
    try:
        g2[int(guid, 16)] = (fqn, kind)
    except ValueError:
        pass


def walk(v, path, acc):
    if isinstance(v, dict):
        if "k" in v and "v" in v and isinstance(v["k"], str):
            if v["k"].startswith("@qst."):
                acc.append((path + "{k}", (v["k"][1:], "Quest")))
            walk(v["v"], path + "{v}", acc)
            return
        for k, x in v.items():
            walk(x, path + "." + k, acc)
    elif isinstance(v, str) and v.startswith("@qst."):
        acc.append((path, (v[1:], "Quest")))
    elif isinstance(v, list):
        for x in v:
            walk(x, path + "[]", acc)
    elif isinstance(v, int) and not isinstance(v, bool):
        u = v & 0xFFFFFFFFFFFFFFFF
        if u in g2:
            acc.append((path, g2[u]))


os.makedirs(out, exist_ok=True)
n = 0
with open(qst) as f, open(os.path.join(out, "refs.tsv"), "w") as w:
    for line in f:
        r = json.loads(line)
        acc = []
        walk(r.get("v", {}), "", acc)
        for path, (tf, tk) in acc:
            if tf == r["fqn"]:
                continue
            w.write(f"{r['fqn']}\t{path}\t{tf}\t{tk}\n")
            n += 1
print("refs", n)
