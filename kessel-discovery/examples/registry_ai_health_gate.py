"""Test the AI health gate on package ability entries (40000002eeb6a660) against combat logs.

usage: python3 -I registry_ai_health_gate.py <apkg.jsonl> <spice.sqlite> <log files...>

apkg.jsonl is dump_objects_jsonl output for prefixes apn. apc. Each package ability entry
(f4787f5b, keyed by ability GUID) may carry a gate g in eeb6a660 (-1 = none). Abilities whose
every entry carries the same g > 0 are tested: each NPC or companion activation of such an
ability in the logs (ability id >> 32 = string_id, reflection 01a17a6a) is compared with the
caster's own health percentage on the same line. Prints, per gate value, how many activations
came at or below the gate and how many above.
"""
import collections
import json
import re
import sqlite3
import sys

LINE = re.compile(r"^\[[^\]]*\] \[([^\]]*)\] \[[^\]]*\] \[([^\]]*)\] \[([^\]]*)\]")
HP = re.compile(r"\((\d+)/(\d+)\)$")


def main():
    gates = collections.defaultdict(set)
    for line in open(sys.argv[1]):
        r = json.loads(line)
        for e in r["fields"].get("f4787f5b") or []:
            gates[e["k"]].add(e["v"].get("eeb6a660", -1))
    db = sqlite3.connect(sys.argv[2])
    by_sid = collections.defaultdict(set)
    for guid, sid in db.execute("select guid, string_id from objects where kind='Ability'"):
        g = gates.get(int(guid, 16))
        if g and sid:
            by_sid[sid] |= g
    tested = {sid: next(iter(g)) for sid, g in by_sid.items() if len(g) == 1 and next(iter(g)) > 0}
    ungated = {sid for sid, g in by_sid.items() if g == {-1}}
    below = collections.Counter()
    above = collections.Counter()
    examples = collections.defaultdict(list)
    ungated_hp = collections.Counter()
    for path in sys.argv[3:]:
        with open(path, errors="replace") as f:
            for raw in f:
                m = LINE.match(raw)
                if not m or "AbilityActivate" not in m[3]:
                    continue
                src = m[1]
                if src.startswith("@") and "/" not in src.split("|")[0]:
                    continue  # players
                ids = re.findall(r"\{(\d+)\}", m[2])
                hp = HP.search(src)
                if not ids or not hp or int(hp[2]) == 0:
                    continue
                sid = int(ids[0]) >> 32
                pct = 100 * int(hp[1]) / int(hp[2])
                if sid in tested:
                    g = tested[sid]
                    if pct <= g + 0.5:
                        below[g] += 1
                    else:
                        above[g] += 1
                        if len(examples[g]) < 3:
                            examples[g].append((m[2].split(" {")[0], round(pct, 1)))
                elif sid in ungated:
                    ungated_hp[pct <= 50] += 1
    for g in sorted(set(below) | set(above)):
        print(f"gate {g:3}: at or below {below[g]:5}, above {above[g]:4}", examples[g])
    print("total at or below", sum(below.values()), "above", sum(above.values()))
    print("ungated abilities: activations at <=50% health", ungated_hp[True], "above", ungated_hp[False])


if __name__ == "__main__":
    main()
