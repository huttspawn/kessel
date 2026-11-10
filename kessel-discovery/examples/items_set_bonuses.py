"""Set bonuses (#331): itmSetBonusesPrototype joined to items and to bonus ability text.

usage: python3 -I items_set_bonuses.py <Item.jsonl> <itmSetBonusesPrototype.json> <spice.sqlite> [out.jsonl]

itmSetBonuses (2d7b877e) maps a set id to {itmSetBonusBonuses 2d7b8781: pieces -> ability,
itmSetBonusDisplayName ee6dc8ea (stringref, id2 << 32 | id1), itmSetBonusItemCount ee6dc8ed,
itmSetBonusBitIndex 5c22e212, itmSetBonusConditionList f1fea450 (cnd refs), itmSetBonusSlots
f1ff1980 (conSlot members that count)}. An item's itmEquipSetBonus (2d7b8780) is a set id.
The optional out file gets one line per (set, pieces, ability) with the effect text.
"""
import collections
import json
import sqlite3
import sys

items_path, sb_path, db_path = sys.argv[1:4]
out_path = sys.argv[4] if len(sys.argv) > 4 else None
sets = {e["k"]: e["v"] for e in json.load(open(sb_path))["2d7b877e"]}
members = collections.defaultdict(list)
n_items = 0
for line in open(items_path):
    o = json.loads(line)
    s = o["v"].get("2d7b8780")
    if s is not None:
        n_items += 1
        members[s].append(o["fqn"])
print("sets", len(sets), "items with itmEquipSetBonus", n_items,
      "resolving", sum(len(v) for k, v in members.items() if k in sets),
      "sets with member items", sum(1 for s in sets if members.get(s)))
db = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)


def text(stringref):
    r = db.execute("select text from strings where id2=? and id1=? and locale='en-us'",
                   (stringref >> 32, stringref & 0xFFFFFFFF)).fetchone()
    return r[0] if r else None


def effect(fqn):
    r = db.execute("select s.text, s.text_raw from objects o join strings s on s.id2=o.string_id "
                   "and s.id1=1 and s.locale='en-us' where o.fqn=? and o.string_id is not null limit 1",
                   (fqn,)).fetchone()
    return (r[1] or r[0]) if r else None


pieces = collections.Counter()
have = total = templ = 0
out = open(out_path, "w") if out_path else None
for sid, v in sets.items():
    name = text(v["ee6dc8ea"])
    for kv in v.get("2d7b8781", []):
        pieces[kv["k"]] += 1
        fqn = kv["v"][1:] if isinstance(kv["v"], str) else None
        t = effect(fqn) if fqn else None
        total += 1
        have += t is not None and t != ""
        templ += bool(t and "<<" in t)
        if out:
            out.write(json.dumps({"set_id": sid, "set_name": name, "pieces": kv["k"], "ability": fqn,
                                  "effect_text": t, "set_items": len(members.get(sid, [])),
                                  "item_count": v.get("ee6dc8ed"),
                                  "slots": [x["enum"] for x in v.get("f1ff1980", [])],
                                  "conditions": v.get("f1fea450", [])}) + "\n")
print("bonus rows", total, "with effect text", have, "of which templated <<N>>", templ)
print("pieces needed", sorted(pieces.items()))
