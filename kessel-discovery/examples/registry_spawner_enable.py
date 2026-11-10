"""Cross spnAutoStart (40000000293dbe27) with the quest-step spawner switches (map 4000000a731cd8be).

usage: python3 -I registry_spawner_enable.py <Spawn.jsonl> <Quest.jsonl> <Encounter.jsonl>

The jsonl files are dump_objects_jsonl output with low-32 field keys ("v"). A quest step's map
731cd8be is keyed by Spawn or Encounter FQN; its value holds 5b20eaaa (enable) or 3aef5105
(disable). For every spawner, counts whether it carries spnAutoStart and whether any quest step
enables or disables it, directly or through an encounter whose slots name it.
"""
import collections
import json
import sys


def keys_with(v, field, flag, out):
    if isinstance(v, dict):
        m = v.get(field)
        if isinstance(m, list):
            for e in m:
                if isinstance(e, dict) and isinstance(e.get("v"), dict) and e["v"].get(flag):
                    out.add(str(e["k"]).lstrip("@"))
        for x in v.values():
            keys_with(x, field, flag, out)
    elif isinstance(v, list):
        for x in v:
            keys_with(x, field, flag, out)


def main():
    spawns = {}
    for line in open(sys.argv[1]):
        r = json.loads(line)
        spawns[r["fqn"]] = bool(r["v"].get("293dbe27"))
    enabled, disabled = set(), set()
    for line in open(sys.argv[2]):
        if "731cd8be" not in line:
            continue
        v = json.loads(line)["v"]
        keys_with(v, "731cd8be", "5b20eaaa", enabled)
        keys_with(v, "731cd8be", "3aef5105", disabled)
    # Encounters: slot map 3d9a52ab9 holds spawner FQNs; switching an encounter switches its spawners.
    enc_members = collections.defaultdict(set)
    for line in open(sys.argv[3]):
        r = json.loads(line)
        for e in r["v"].get("d9a52ab9") or []:
            if isinstance(e, dict) and isinstance(e.get("v"), str):
                enc_members[r["fqn"]].add(e["v"].lstrip("@"))
    for enc in list(enabled):
        enabled |= enc_members.get(enc, set())
    for enc in list(disabled):
        disabled |= enc_members.get(enc, set())
    table = collections.Counter()
    for guid, auto in spawns.items():
        table["autostart" if auto else "no autostart", "quest-enabled" if guid in enabled else "not enabled"] += 1
    for k in sorted(table):
        print(k, table[k])
    print("spawn keys enabled", len(enabled & set(spawns)), "disabled", len(disabled & set(spawns)))


if __name__ == "__main__":
    main()
