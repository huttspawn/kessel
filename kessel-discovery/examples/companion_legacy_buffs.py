"""Companion legacy unlock buffs: what each abl.legacy.companion_unlock.* ability modifies.

usage: python3 -I companion_legacy_buffs.py <abl_all.jsonl>

For every effect variant under abl.legacy.companion_unlock., prints ModifyStat actions (STAT name from the
1-based effParam_Stat, modStatType, amount) and any ability the effect calls or applies, to show the per-companion
unlocks and the five archetype stat buffs they grant (e.g. +1% accuracy behind the 110% target).
"""
import collections
import json
import sys

enums = {e["name"]: e["members"] for e in json.load(open("kessel/resources/gom_enums.json"))}
P, A, S, M = enums["effParam"], enums["effAction"], enums["STAT"], enums["modStatType"]


def params(entry):
    d = {}
    for f in ("40000004e251d1d3", "40000004e251d1d4", "40000004e251d1d5", "40000004e251d1d6", "40000004e251d1d7", "40000004e27e11d1"):
        for kv in entry.get(f, []):
            if isinstance(kv, dict) and isinstance(kv.get("k"), dict):
                d[P[kv["k"]["enum"]].replace("effParam_", "")] = kv["v"]
    return d


out = collections.defaultdict(set)
for line in open(sys.argv[1]):
    if '"fqn":"abl.legacy.companion_unlock.' not in line:
        continue
    o = json.loads(line)

    def walk(x):
        if isinstance(x, dict):
            if "40000004e251d1cc" in x:
                act = A[x["40000004e251d1cc"]["enum"]].replace("effAction_", "")
                p = params(x)
                if act == "ModifyStat" and p.get("Stat"):
                    amt = p.get("AmountMin") or p.get("AmountPercent")
                    out[o["fqn"]].add(f"{S[p['Stat'] - 1]} {M[p['StatModType'] - 1] if p.get('StatModType') else ''} {amt}")
                refs = [v for v in p.values() if isinstance(v, str) and v.startswith("@abl.")]
                for r in refs:
                    out[o["fqn"]].add(f"{act} -> {r[1:]}")
            for y in x.values():
                walk(y)
        elif isinstance(x, list):
            for y in x:
                walk(y)

    walk(o["v"])
for fqn in sorted(out):
    print(fqn)
    for s in sorted(out[fqn]):
        print("   ", s)
