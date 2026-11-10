"""Which companion belongs to which legacy bonus group.

The group names (healer, melee damage, melee tank, ranged damage, ranged tank) are pre-4.0 role names kept
in the legacy-unlock texts; since 4.0 every companion takes any combat role, so they are labels of legacy
bonus groups, not companion roles. Whether the server still grants the group buffs is not in the client
(RESEARCH 01a1c30d-73cd rev 6).

usage: python3 -I companion_archetypes.py <lgcUnlockPrototypeMap.json> <unlocks.stb dump>

lgcUnlockPrototypeMap.json is dump_singleton output; the stb dump is
`dump_stb <assets> <hashes> /resources/en-us/str/lgc/unlocks.stb` (lines path|id1|id2|text).
A companion unlock row (its quest list 9ca4acd0 names qst.companion.*) carries 74cafaa0 = the id1 of
its name and 74cafaa2 = the id1 of its description in str/lgc/unlocks.stb. The five descriptions
447-451 each name one archetype and the stat it raises, which is the stat of one
abl.legacy.companion_unlock.archetype.*.stat_buff ability.
"""
import collections
import json
import sys

rows = json.load(open(sys.argv[1]))["d5d3a3da"]
text = {}
for line in open(sys.argv[2], encoding="utf-8", errors="replace"):
    parts = line.rstrip("\n").split("|", 3)
    if len(parts) == 4 and parts[1].isdigit():
        text[int(parts[1])] = parts[3]
ARCH = {447: "healer (healing received)", 448: "melee_dps (critical damage)", 449: "melee_tank (accuracy)",
        450: "ranged_dps (critical chance)", 451: "ranged_tank (max health)"}
by = collections.defaultdict(list)
for r in rows:
    v = r["v"]
    quests = v.get("9ca4acd0", [])
    if not any("qst.companion." in q for q in quests):
        continue
    desc = v.get("74cafaa2")
    name = text.get(v.get("74cafaa0"), v.get("80be853b"))
    cls = quests[0].split(".")[3] if quests else "?"
    by[desc].append((cls, name.strip()))
for desc in sorted(by, key=lambda d: (d is None, d)):
    print(f"{desc} {ARCH.get(desc, '?')}: {text.get(desc, '')[:70]}")
    for cls, name in sorted(by[desc]):
        print(f"    {cls:16} {name}")
