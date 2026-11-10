"""Every ModifyStat on an accuracy stat (melee, ranged, force, tech accuracy; 1-based effParam_Stat), by ability.

usage: python3 -I accuracy_passives.py <abl_all.jsonl>

Prints each ability FQN, the stat, modStatType and amount, grouped by FQN prefix, to find a +1% accuracy
that every player has (a class or discipline passive) behind the community's 110% target.
"""
import collections
import json
import sys

enums = {e["name"]: e["members"] for e in json.load(open("kessel/resources/gom_enums.json"))}
P, A, S, M = enums["effParam"], enums["effAction"], enums["STAT"], enums["modStatType"]
ACC = {S.index(n) + 1 for n in ("STAT_cbt_melee_accuracy", "STAT_cbt_ranged_accuracy", "STAT_cbt_force_accuracy",
                                 "STAT_cbt_tech_accuracy", "STAT_cbtAccuracy")}


def params(entry):
    d = {}
    for f in ("40000004e251d1d3", "40000004e251d1d5", "40000004e251d1d6", "40000004e27e11d1"):
        for kv in entry.get(f, []):
            if isinstance(kv, dict) and isinstance(kv.get("k"), dict):
                d[P[kv["k"]["enum"]].replace("effParam_", "")] = kv["v"]
    return d


hits = collections.defaultdict(list)


def walk(x, fqn):
    if isinstance(x, dict):
        if "40000004e251d1cc" in x and A[x["40000004e251d1cc"]["enum"]] == "effAction_ModifyStat":
            p = params(x)
            if p.get("Stat") in ACC:
                hits[fqn].append((S[p["Stat"] - 1], M[p["StatModType"] - 1] if p.get("StatModType") else None,
                                  {k: p[k] for k in ("AmountMin", "AmountMax", "AmountPercent") if p.get(k)}))
        for y in x.values():
            walk(y, fqn)
    elif isinstance(x, list):
        for y in x:
            walk(y, fqn)


for line in open(sys.argv[1]):
    if "e251d1cc" not in line:
        continue
    o = json.loads(line)
    walk(o["v"], o["fqn"])
print("abilities with an accuracy ModifyStat:", len(hits))
pre = collections.Counter(".".join(f.split(".")[:3]) for f in hits)
print(pre.most_common(25))
for f in sorted(hits):
    if f.startswith(("abl.player", "abl.sith", "abl.jedi", "abl.bounty", "abl.trooper", "abl.smuggler", "abl.agent",
                     "abl.imperial", "abl.republic", "abl.generic", "abl.class", "abl.discipline", "abl.legacy",
                     "abl.companion", "abl.common")):
        amounts = sorted({json.dumps(h, sort_keys=True) for h in hits[f]})
        print(f, amounts[:4])
