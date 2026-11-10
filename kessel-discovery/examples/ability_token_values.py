"""Resolve ability description tokens <<N>> to the effect they read (#320).

usage: python3 -I ability_token_values.py <abl_all.jsonl> <out tokens.jsonl>

abl_all.jsonl is every Ability row, canonical and effect variants, from residue_dump.
The canonical row's ablDescriptionTokens list (384b793a) holds one entry per <<N>>, in order:
ablDescriptionTokenType (384b7939), ablDescriptionTokenMultiplier (384b793c),
ablDescriptionTokenEffect (a38ca337) and ablDescriptionTokenSubEffect (a38ca338).
TokenEffect is an effNumber (2efdc39f) of an effect variant row with the same FQN.
A Duration token's value is multiplier x effDuration (2f5d7053, ms) / 1000.
For Stat, Damage, Healing and Absorption tokens the line lists the effect's amount
parameters; those scale at run time (StandardRatingPercent, StandardHealthPercent).
effInitializer, effAction and effCondition int params naming an enum are 1-based
(effParam_Stat 155 = STAT member 154, rtg_attack_power; StatModType 9 = modStatType
member 8, StandardRatingPercent).
"""
import collections
import json
import sys

src, dst = sys.argv[1:3]
enums = {e["name"]: e["members"] for e in json.load(open("kessel/resources/gom_enums.json"))}
P, A, TT = enums["effParam"], enums["effAction"], enums["ablDescriptionTokenType"]
TRIG = enums["effTrigger"]
canon, effects = {}, collections.defaultdict(dict)
for line in open(src):
    o = json.loads(line)
    v = o["v"]
    if "4000000a384b793a" in v:
        canon.setdefault(o["fqn"], v["4000000a384b793a"])
    n = v.get("400000002efdc39f")
    if n is not None:
        effects[o["fqn"]][n] = v


def params(entry):
    d = {}
    for f in ("40000004e251d1d3", "40000004e251d1d5", "40000004e251d1d6", "40000004e27e11d1"):
        for kv in entry.get(f, []):
            if isinstance(kv, dict) and isinstance(kv.get("k"), dict):
                d[P[kv["k"]["enum"]].replace("effParam_", "")] = kv["v"]
    return d


def actions(v):
    out = []

    def walk(x):
        if isinstance(x, dict):
            if "40000004e251d1cc" in x:
                out.append((A[x["40000004e251d1cc"]["enum"]].replace("effAction_", ""), params(x)))
            for y in x.values():
                walk(y)
        elif isinstance(x, list):
            for y in x:
                walk(y)

    walk(v.get("40000004e251d1b2", []))
    return out


AMOUNTS = ("AmountMin", "AmountMax", "AmountPercent", "Coefficient", "StandardRatingPercentMin",
           "StandardHealthPercentMin", "StandardHealthPercentMax", "Stat", "StatModType")
tally = collections.Counter()
with open(dst, "w") as w:
    for fqn, toks in canon.items():
        for i, t in enumerate(toks):
            ty = TT[t["4000000a384b7939"]["enum"]].replace("ablDescriptionTokenType", "") if "4000000a384b7939" in t else None
            mult = t.get("4000000a384b793c", 1.0)
            num = t.get("4000000ba38ca337")
            eff = effects.get(fqn, {}).get(num)
            rec = {"fqn": fqn, "token": i + 1, "type": ty, "multiplier": mult, "effect": num,
                   "sub_effect": t.get("4000000ba38ca338"), "effect_found": eff is not None}
            if eff is not None:
                rec["effect_duration_ms"] = eff.get("400000002f5d7053")
                acts = actions(eff)
                if ty == "Duration" and rec["effect_duration_ms"] is not None:
                    rec["value"] = mult * rec["effect_duration_ms"] / 1000.0
                    rec["unit"] = "seconds"
                elif ty in ("Stat", "Damage", "Healing", "Absorption"):
                    rec["amounts"] = [{a: {k: p[k] for k in AMOUNTS if p.get(k)}} for a, p in acts
                                      if a in ("ModifyStat", "Damage", "SpellDamage", "WeaponDamage", "Heal", "AbsorbDamage")]
            tally[(ty, eff is not None, "value" in rec)] += 1
            w.write(json.dumps(rec) + "\n")
print("canonical abilities with tokens", len(canon))
for (ty, found, valued), n in sorted(tally.items(), key=lambda x: -x[1]):
    print(f"{n:6}  type={ty} effect_found={found} value={valued}")
