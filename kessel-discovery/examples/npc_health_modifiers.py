"""NPC max-health passives: every ModifyStat on STAT_cbt_health_max (effParam_Stat 8, 1-based).

usage: python3 -I npc_health_modifiers.py <abl_all.jsonl> <out.json>

Writes [fqn, modStatType, {AmountPercent|AmountMin|...}] per action and prints a summary.
Then checks the boss pairs below against max HP read from Sean's combat logs: a boss's max HP
is base x (1 + AmountPercent of its difficulty passive), so the base must come out equal.
"""
import collections
import json
import sys

src, dst = sys.argv[1:3]
enums = {e["name"]: e["members"] for e in json.load(open("kessel/resources/gom_enums.json"))}
P, A, MOD = enums["effParam"], enums["effAction"], enums["modStatType"]


def params(entry):
    d = {}
    for f in ("40000004e251d1d3", "40000004e251d1d5", "40000004e251d1d6", "40000004e27e11d1"):
        for kv in entry.get(f, []):
            if isinstance(kv, dict) and isinstance(kv.get("k"), dict):
                d[P[kv["k"]["enum"]].replace("effParam_", "")] = kv["v"]
    return d


hits = []


def walk(x, fqn):
    if isinstance(x, dict):
        if "40000004e251d1cc" in x and A[x["40000004e251d1cc"]["enum"]] == "effAction_ModifyStat":
            p = params(x)
            if p.get("Stat") == 8:
                mod = MOD[p["StatModType"] - 1] if p.get("StatModType") else None
                hits.append([fqn, mod, {k: p[k] for k in ("AmountMin", "AmountMax", "AmountPercent", "Coefficient") if p.get(k)}])
        for y in x.values():
            walk(y, fqn)
    elif isinstance(x, list):
        for y in x:
            walk(y, fqn)


for line in open(src):
    o = json.loads(line)
    walk(o["v"], o["fqn"])
json.dump(hits, open(dst, "w"))
print("ModifyStat on STAT_cbt_health_max:", len(hits), collections.Counter(h[1] for h in hits))
pct = {h[0]: h[2].get("AmountPercent", 0.0) for h in hits}
# (passive ability, max HP in the log) for the same boss at two difficulties
PAIRS = [
    ("abl.npc.ability.custom.flashpoint.eternity_vault.soa.difficulty_1.passive_immunity", 6628171),
    ("abl.npc.ability.custom.flashpoint.eternity_vault.soa.difficulty_3.passive_immunity", 13872915),
    ("abl.npc.ability.custom.flashpoint.eternity_vault.gharj.difficulty_1.gharj_hitpoints", 12331480),
    ("abl.npc.ability.custom.flashpoint.eternity_vault.gharj.difficulty_3.gharj_hitpoints", 29749696),
    ("abl.qtr.1x1.raid.karaggas_palace.enemy.boss.boss_5_karagga.karagga.difficulty_1.karagga_hitpoints", 16647499),
]
for fqn, hp in PAIRS:
    p = pct.get(fqn)
    print(f"{fqn.split('.')[-3]}.{fqn.split('.')[-2]}.{fqn.split('.')[-1]}: log {hp}, AmountPercent {p:.4f}, "
          f"base = log / (1 + p) = {hp / (1 + p):,.1f}")
