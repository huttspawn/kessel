"""Walk NPC -> (npc apn list c802b2a8, class apn 1d72ea3a) -> apn abilities (f4787f5b keys)
-> max-health passives, for NPCs seen in combat logs, and test max HP = base x (1 + sum p).

usage: python3 -I npc_health_walk.py <Npc.jsonl> <class.jsonl> <apn.jsonl> <hpmods.json> <npc_maxhp.json> [name ...]

Npc.jsonl, apn.jsonl: dump_kind_jsonl <unfiltered> Npc / AnimationPackage. class.jsonl: residue_dump of
class rows. hpmods.json: npc_health_modifiers.py output. npc_maxhp.json: {log name id: {name, maxhp:
{value: lines}}} from Sean's logs. For each NPC name given (default: every logged NPC with a health
passive), prints toughness, level, the passives reached and the logged max HPs divided by (1 + sum p).
"""
import collections
import json
import sys

npc_path, class_path, apn_path, hp_path, log_path, *names = sys.argv[1:]
cls = {}
for line in open(class_path):
    o = json.loads(line)
    cls[o["fqn"]] = o["v"]
apn = {}
for line in open(apn_path):
    o = json.loads(line)
    apn[o["fqn"]] = o["v"]
mods = collections.defaultdict(list)
for fqn, mod, amt in json.load(open(hp_path)):
    mods[fqn].append((mod, amt))
logs = json.load(open(log_path))


def apn_abilities(name, seen):
    if name in seen or name not in apn:
        return []
    seen.add(name)
    out = []
    v = apn[name]
    for kv in v.get("f4787f5b", []):
        k = kv["k"]
        if isinstance(k, str) and k.startswith("@abl."):
            out.append(k[1:])
    for kv in v.get("416ac702", []):
        x = kv["v"]
        if isinstance(x, str) and x.startswith("@abl.") and x[1:] not in out:
            out.append(x[1:])
        if isinstance(x, str) and x.startswith("@apn."):
            out += apn_abilities(x[1:], seen)
    return out


by_logid = collections.defaultdict(list)
for line in open(npc_path):
    o = json.loads(line)
    v = o["v"]
    for slot in v.get("a787ee87", []):
        sv = slot.get("v")
        if isinstance(sv, dict) and sv.get("5ce87489") == "str.npc":
            by_logid[str(sv.get("5ce87488"))].append((o["fqn"], v))
            break
for logid, entry in logs.items():
    if names and entry["name"] not in names:
        continue
    for fqn, v in by_logid.get(logid, []):
        apns = [a[1:] for a in v.get("c802b2a8", []) if isinstance(a, str)]
        c = v.get("731cd8c0")
        cv = cls.get(c[1:], {}) if isinstance(c, str) else {}
        ca = cv.get("4000000a1d72ea3a")
        if isinstance(ca, str):
            apns.append(ca[1:])
        abls = []
        for a in apns:
            abls += apn_abilities(a, set())
        found = [(a, m) for a in abls for m in mods.get(a, [])]
        if not found and names == []:
            continue
        # One ability lists one effect variant per difficulty; only one applies, so each
        # variant is tested on its own, never summed.
        print(f"{entry['name']} | {fqn} | toughness {v.get('618ec27c', {}).get('enum')} level {v.get('27bda14b')}"
              f" | log max HP {entry['maxhp']}")
        print(f"   apns {apns}")
        for a, (mod, amt) in found:
            p = amt.get("AmountPercent", 0.0)
            flat = amt.get("AmountMin", 0.0)
            bases = {h: round((int(h) - flat) / (1 + p), 1) for h in entry["maxhp"]} if p > -1 else {}
            print(f"   variant {a}: {mod} {amt} -> (log - flat) / (1 + p): {bases}")
