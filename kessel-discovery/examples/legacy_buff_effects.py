"""Every effect action and condition of the legacy class-story and companion-group buffs.

usage: python3 -I legacy_buff_effects.py <abl_all.jsonl> <gom_enums.json> [fqn-prefix ...]

Default prefixes: abl.legacy.buff_unlock. (the eight Chapter 2 class-story buffs, paired into four tokens),
abl.legacy.class_unlock. (the eight Chapter 3 heroic abilities) and abl.legacy.companion_unlock.archetype.
(the five companion-group buffs). For each object it prints its initializers, its conditions
(effCondition_*) and each action with named params (ModifyStat stat names, modStatType, amounts).
An object with no condition line has none: its actions apply whenever the effect is on the character.
"""
import json
import sys

enums = {e["name"]: e["members"] for e in json.load(open(sys.argv[2]))}
P, A, C, I, T = enums["effParam"], enums["effAction"], enums["effCondition"], enums["effInitializer"], enums["effTrigger"]
S, M = enums["STAT"], enums["modStatType"]
PREFIXES = tuple(sys.argv[3:]) or ("abl.legacy.buff_unlock.", "abl.legacy.class_unlock.",
                                   "abl.legacy.companion_unlock.archetype.")
KIND = {"40000004e251d1cc": ("action", A), "40000004e251d1cd": ("condition", C),
        "40000004e251d1d0": ("init", I), "40000004e251d1cf": ("trigger", T)}
PARAM_FIELDS = ("40000004e251d1d3", "40000004e251d1d4", "40000004e251d1d5", "40000004e251d1d6",
                "40000004e251d1d7", "40000004e27e11d1")


def params(entry):
    d = {}
    for f in PARAM_FIELDS:
        for kv in entry.get(f, []):
            if isinstance(kv, dict) and isinstance(kv.get("k"), dict) and "enum" in kv["k"]:
                v = kv["v"]
                if v in (0, 0.0, "", False, [], {}):
                    continue
                d[P[kv["k"]["enum"]].replace("effParam_", "")] = v
    if isinstance(d.get("Stat"), int) and 0 < d["Stat"] <= len(S):
        d["Stat"] = S[d["Stat"] - 1]
    if isinstance(d.get("StatModType"), int) and 0 < d["StatModType"] <= len(M):
        d["StatModType"] = M[d["StatModType"] - 1]
    return d


def walk(x, out):
    if isinstance(x, dict):
        for k, (kind, names) in KIND.items():
            if k in x and isinstance(x[k], dict) and "enum" in x[k]:
                out.append((kind, names[x[k]["enum"]], params(x)))
        for y in x.values():
            walk(y, out)
    elif isinstance(x, list):
        for y in x:
            walk(y, out)


for line in open(sys.argv[1]):
    if '"fqn":"abl.legacy.' not in line:
        continue
    o = json.loads(line)
    if not o["fqn"].startswith(PREFIXES):
        continue
    out = []
    walk(o["v"], out)
    if not out:
        continue
    conds = [e for e in out if e[0] in ("condition", "trigger")]
    print("=====", o["fqn"], o.get("guid"), "conditions/triggers:", len(conds))
    for kind, name, p in out:
        if kind == "init" and name not in ("effInitializer_SetPassive", "effInitializer_SetPersistsAfterDeath",
                                           "effInitializer_SetType", "effInitializer_SetDuration"):
            continue
        print("  ", kind, name, json.dumps(p, sort_keys=True))
