"""Relic proc chance, internal cooldown, duration and magnitude parameters, from the data (#308).

usage: python3 -I relic_proc_values.py <abl_all.jsonl> [fqn prefix, default abl.itm.relic.]

For every ability under the prefix, walks its effect variants and prints, per effect number:
effDuration (ms), trigger ProcChancePercent and RateLimitInterval (the internal cooldown, ms),
and each ModifyStat / Damage / Heal / AbsorbDamage action's amount parameters. Live relics all
grant one of 16 abl.itm.relic.*.scales_with_item_rating abilities, whose amounts are
StandardRatingPercent (modStatType 9) instead of a flat AmountMin/AmountMax.
"""
import collections
import json
import sys

src = sys.argv[1]
prefix = sys.argv[2] if len(sys.argv) > 2 else "abl.itm.relic."
enums = {e["name"]: e["members"] for e in json.load(open("kessel/resources/gom_enums.json"))}
P, A, STAT, MOD = enums["effParam"], enums["effAction"], enums["STAT"], enums["modStatType"]


def params(entry):
    d = {}
    for f in ("40000004e251d1d3", "40000004e251d1d5", "40000004e251d1d6", "40000004e27e11d1"):
        for kv in entry.get(f, []):
            if isinstance(kv, dict) and isinstance(kv.get("k"), dict):
                d[P[kv["k"]["enum"]].replace("effParam_", "")] = kv["v"]
    return d


rows = collections.defaultdict(dict)
for line in open(src):
    if f'"fqn":"{prefix}' not in line:
        continue
    o = json.loads(line)
    n = o["v"].get("400000002efdc39f")
    if n is None:
        continue
    out = {"duration_ms": o["v"].get("400000002f5d7053"), "chance": [], "icd_ms": [], "amounts": []}

    def walk(x):
        if isinstance(x, dict):
            p = params(x)
            if "ProcChancePercent" in p and p["ProcChancePercent"]:
                out["chance"].append(p["ProcChancePercent"])
            if p.get("RateLimitInterval"):
                out["icd_ms"].append(p["RateLimitInterval"])
            act = A[x["40000004e251d1cc"]["enum"]] if "40000004e251d1cc" in x else None
            if act in ("effAction_ModifyStat", "effAction_SpellDamage", "effAction_Damage", "effAction_Heal", "effAction_AbsorbDamage"):
                amt = {k: p[k] for k in ("AmountMin", "AmountMax", "AmountPercent", "StandardRatingPercentMin") if p.get(k)}
                if "Stat" in p:
                    amt["stat"] = STAT[p["Stat"] - 1]
                if p.get("StatModType"):
                    amt["mod"] = MOD[p["StatModType"] - 1]
                out["amounts"].append({act.replace("effAction_", ""): amt})
            for y in x.values():
                walk(y)
        elif isinstance(x, list):
            for y in x:
                walk(y)

    walk(o["v"])
    rows[o["fqn"]][n] = out
for fqn in sorted(rows):
    if "scales_with_item_rating" not in fqn and len(sys.argv) <= 2:
        continue
    print(fqn)
    for n in sorted(rows[fqn]):
        r = rows[fqn][n]
        print(f"   effect {n}: duration {r['duration_ms']} ms, chance {sorted(set(r['chance']))}, "
              f"rate limit {sorted(set(r['icd_ms']))} ms, amounts {r['amounts']}")
