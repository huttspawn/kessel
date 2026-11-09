"""Decode collections: entry -> name, items, abilities (pets/mounts/toys), titles,
and source -> entries. Writes dec-economy/collections.json and prints coverage."""
import os
import collections, json
D = os.environ["KESSEL_SCRATCH"] + "/dec-economy"  # dumps from dump_kind_jsonl, dump_singleton, dump_stb
items = json.load(open(D + "/sg/colCollectionItemsPrototype.json"))
srcs = json.load(open(D + "/sg/colCollectionSourcesPrototype.json"))
cats = json.load(open(D + "/sg/colCollectionCategoriesPrototype.json"))
stb = {}
for l in open(D + "/stb/collection.txt"):
    p = l.rstrip("\n").split("|", 3)
    if len(p) == 4:
        stb[(int(p[2]) << 32) | int(p[1])] = p[3]
# mtxstorefrontitems for entry names
for f in ("mtxstorefrontitems",):
    for l in open(D + f"/stb/{f}.txt"):
        p = l.rstrip("\n").split("|", 3)
        if len(p) == 4:
            stb[(int(p[2]) << 32) | int(p[1])] = p[3]

entries = {}
for row in items["0344aaf8"]:
    v = row["v"]
    entries[row["k"]] = {
        "name_id": v.get("0344aaf2"),
        "image": v.get("0344aaf4"),
        "items": v.get("a2180abd", []),
        "display_items": v.get("e4129b18", []),
        "abilities": v.get("a21d88f9", []),
        "mounts": v.get("16d0a292", []),
        "titles": v.get("a21d88fb", []),
        "flair": v.get("d932b8ea", []),
        "quests": v.get("d94ee630", []),
        "in_store": v.get("c7665d8a"),
        "limited": v.get("c7665d8b"),
        "min_level": v.get("c768ce88"),
        "bit": v.get("a2180ab9"),
    }
print("entries", len(entries))
cnt = collections.Counter()
for e in entries.values():
    for f in ("items", "abilities", "mounts", "titles", "flair", "quests"):
        if e[f]:
            cnt[f] += 1
    if any(isinstance(x, str) and x.startswith("@") for x in e["items"]):
        cnt["items_resolve"] += 1
print("entries with", dict(cnt))
ab = collections.Counter(a.split(".")[2] if isinstance(a, str) and a.startswith("@abl.") else "raw" for e in entries.values() for a in e["abilities"])
print("ability families", ab.most_common(8))
source_rows = []
for row in srcs["1c69dd7e"]:
    v = row["v"]
    ids = [x["k"] for x in v.get("1c69dd83", [])]
    source_rows.append({"source_id": row["k"], "name_id": v.get("1c69dd7f"), "name": stb.get(v.get("1c69dd7f")),
                        "image": v.get("1c69dd81"), "entries": ids})
joined = sum(1 for s in source_rows for i in s["entries"] if i in entries)
total = sum(len(s["entries"]) for s in source_rows)
named = sum(1 for s in source_rows if s["name"])
print("sources", len(source_rows), "named from collection.stb", named, "source->entry links", total, "resolving", joined)
covered = {i for s in source_rows for i in s["entries"]}
print("entries with at least one source", len(covered & set(entries)))
for s in source_rows[:62]:
    print(" ", s["source_id"], s["name"], s["image"], len(s["entries"]))
json.dump({"entries": {str(k): v for k, v in entries.items()}, "sources": source_rows}, open(D + "/collections.json", "w"))
