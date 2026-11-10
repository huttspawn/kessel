#!/usr/bin/env python3
"""Decode push 3, world: explain the areas the map data names with no area file.

Joins, per area id the unknowns report lists ('named by the map data; no area file'):
  - its mapAreasDataProto entry (0027bf7b map: 0027bf79 kind enum, 0027bf7f log id,
    5cd72a38 chat zone, 5cd72a39 number), dumped to mapareas.json;
  - whether a world.areas.<id>.mapdata object exists (map pages) and what its pages are;
  - whether a phase (phs) names it as the area it loads (24ed04cd) or exits to (24ed04cc);
  - which area WITH a file shares its chat zone or log id;
  - the archive-file check from world3_dir_files (area.dat, mapnotes.not, dictionary paths);
  - every other object/singleton payload naming the id (8-byte LE or BE), via a byte scan
    of the extract's payloads.

Usage: world3_area_nofile.py <extract.sqlite> <mapareas.json> <phs.jsonl> <area_named.tsv>
       <nofile_dirs.tsv> <out.tsv>
"""
import base64
import collections
import json
import sqlite3
import struct
import sys

db_p, ma_p, phs_p, named_p, dirs_p, out_p = sys.argv[1:7]
db = sqlite3.connect(db_p)

ma = {}
for e in json.load(open(ma_p))["0027bf7b"]:
    v = e["v"]
    ma[int(e["k"])] = {
        "kind": (v.get("0027bf79") or {}).get("enum"),
        "log": v.get("0027bf7f"),
        "chat": v.get("5cd72a38"),
        "num": v.get("5cd72a39"),
    }
with_file = set()
for line in list(open(named_p))[1:]:
    c = line.split("\t")
    if c[0] != "0":
        with_file.add(int(c[0]))
dirs = {}
for line in list(open(dirs_p))[1:]:
    c = line.rstrip("\n").split("\t")
    dirs[int(c[0])] = c
targets = sorted(dirs)

mapdata = {}
for fqn, js in db.execute("select fqn, json from objects where fqn like 'world.areas.%.mapdata'"):
    try:
        mapdata[int(fqn.split(".")[2])] = len(js)
    except ValueError:
        pass
phs_load = collections.defaultdict(list)
phs_exit = collections.defaultdict(list)
for line in open(phs_p):
    o = json.loads(line)
    v = o["v"]
    for f, sink in (("24ed04cd", phs_load), ("24ed04cc", phs_exit)):
        x = v.get(f)
        if isinstance(x, int):
            sink[x & 0xFFFFFFFFFFFFFFFF].append(o["fqn"])

by_chat = collections.defaultdict(set)
by_log = collections.defaultdict(set)
for i, m in ma.items():
    if i in with_file:
        by_chat[m["chat"]].add(i)
        by_log[m["log"]].add(i)

# byte scan of every payload (objects and singletons) for the target ids
pats = {}
for t in targets:
    pats[struct.pack("<Q", t)] = t
    pats[struct.pack(">Q", t)] = t
named_by = collections.defaultdict(collections.Counter)


def scan(kind, fqn, b64):
    if not b64:
        return
    d = base64.b64decode(b64)
    for p, t in pats.items():
        if p in d:
            named_by[t][(kind, fqn.split(".")[0])] += 1


for kind, fqn, js in db.execute("select kind, fqn, json from objects"):
    try:
        scan(kind, fqn, json.loads(js).get("payload_b64"))
    except (ValueError, AttributeError):
        pass
for fqn, b64 in db.execute("select fqn, payload_b64 from singletons"):
    scan("singleton", fqn, b64)

rows = []
cls = collections.Counter()
for t in targets:
    m = ma.get(t, {})
    share_chat = sorted(by_chat.get(m.get("chat"), set()) - {t}) if m else []
    share_log = sorted(by_log.get(m.get("log"), set()) - {t}) if m else []
    refs = named_by.get(t, {})
    other = {k: n for k, n in refs.items() if k[0] != "singleton" or k[1] != "mapAreasDataProto"}
    if t in phs_load:
        c = "phase loads it"
    elif share_log or share_chat:
        c = "shares chat zone or log id with an area that has a file"
    elif t in mapdata:
        c = "map pages only"
    elif other:
        c = "named by other objects"
    else:
        c = "mapAreasDataProto entry only"
    cls[c] += 1
    rows.append([str(t), c, str(m.get("kind")), str(m.get("log")), str(m.get("chat")), str(m.get("num")),
                 str(t in mapdata), ";".join(phs_load.get(t, [])[:3]), ";".join(phs_exit.get(t, [])[:2]),
                 ",".join(map(str, share_chat[:3])), ",".join(map(str, share_log[:3])),
                 ";".join(f"{k[0]}:{k[1]}x{n}" for k, n in list(other.items())[:4]), dirs[t][3]])
with open(out_p, "w") as f:
    f.write("id\tclass\tkind\tlog_id\tchat\tnum\tmapdata\tphs_loads\tphs_exits\tshares_chat\tshares_log\tnamed_by\tdict_listed\n")
    for r in rows:
        f.write("\t".join(r) + "\n")
print("areas", len(targets), dict(cls))
print("kind enum", collections.Counter(ma.get(t, {}).get("kind") for t in targets))
print("in mapAreasDataProto", sum(1 for t in targets if t in ma), "with mapdata", sum(1 for t in targets if t in mapdata))
print("chat zones", collections.Counter(ma.get(t, {}).get("chat") for t in targets).most_common(25))
