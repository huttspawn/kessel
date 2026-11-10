#!/usr/bin/env python3
"""Decode push 3, world: print one Hydra script's handlers and actions readably.

Resolves each action's values through the script's own named-reference table
(ed10e6a0: key -> {080ce2d2 name, 080ce2f1 id, f21c80b6 kind enum}) so a
'Set Quest State' on a local key shows the quest variable it means.

Usage: world3_hydra_dump.py <Hydra.jsonl> <hyd fqn> [substring to keep]
"""
import json
import sys

path, want = sys.argv[1], sys.argv[2]
keep = sys.argv[3] if len(sys.argv) > 3 else None
ACTION, VALUE = "e5346fa0", "e5346faf"

for line in open(path):
    if f'"fqn":"{want}"' not in line:
        continue
    o = json.loads(line)
    v = o["v"]
    refs = {}

    def collect(x):
        if isinstance(x, dict):
            for kk, xx in x.items():
                if kk == "ed10e6a0" and isinstance(xx, list):
                    for e in xx:
                        if isinstance(e, dict) and isinstance(e.get("v"), dict):
                            refs[e["k"]] = (e["v"].get("080ce2d2"), (e["v"].get("f21c80b6") or {}).get("enum"), e["v"].get("080ce2f1"))
                collect(xx)
        elif isinstance(x, list):
            for xx in x:
                collect(xx)
    collect(v)
    print("named refs:")
    for k, r in refs.items():
        if not keep or keep in str(r[0]):
            print("  ", k, r)

    def acts(x, h):
        if isinstance(x, dict):
            if ACTION in x:
                vals = {kk: xx for kk, xx in x.items() if kk != ACTION and xx not in ([], {}, None)}
                s = json.dumps(vals)
                named = []
                for kk, r in refs.items():
                    if str(kk) in s or (isinstance(r[2], int) and str(r[2]) in s):
                        named.append(r[0])
                line = f"  [{h}] {x[ACTION]} {s[:300]} -> {named}"
                if not keep or keep in line:
                    print(line)
            for xx in x.values():
                acts(xx, h)
        elif isinstance(x, list):
            for xx in x:
                acts(xx, h)
    for hh in v.get("e389e2e3", []) or []:
        acts(hh.get("v"), hh.get("k"))
    break
