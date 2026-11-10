#!/usr/bin/env python3
"""Decode push 3, world: what the map data says about areas that have no area file.

For each world.areas.<id>.mapdata (dump_kind_jsonl kind 'world'): its pages
(b5f9c772: name 6225006b, tags b970d7b7, log area id 858fc65e) with the page's display
name from str.sys.worldmap (858fc65e = 32 << 32 | entry), and whether the same display
names / page names occur in the map data of an area that HAS an area file.

Usage: world3_mapdata_pages.py <extract.sqlite> <world.jsonl> <nofile_class.tsv> <area_named.tsv> <out.tsv>
"""
import collections
import json
import sqlite3
import sys

db_p, world_p, nofile_p, named_p, out_p = sys.argv[1:6]
db = sqlite3.connect(db_p)
wm = {id1: t for id1, t in db.execute("select id1, text from strings where fqn like 'str.sys.worldmap.%' and id2 = 32")}
nofile = {l.split("\t")[0]: l.rstrip("\n").split("\t") for l in list(open(nofile_p))[1:]}
withfile = {l.split("\t")[0] for l in list(open(named_p))[1:] if not l.startswith("0\t")}

areas = {}
for l in open(world_p):
    o = json.loads(l)
    aid = o["fqn"].split(".")[2]
    pages = []
    for p in o["v"].get("b5f9c772") or []:
        lid = p.get("858fc65e")
        disp = wm.get(lid & 0xFFFFFFFF) if isinstance(lid, int) and lid >> 32 == 32 else None
        pages.append((p.get("6225006b"), disp, lid))
    areas[aid] = pages

file_disp = collections.defaultdict(set)
file_pname = collections.defaultdict(set)
for aid in withfile:
    for name, disp, lid in areas.get(aid, []):
        if disp:
            file_disp[disp].add(aid)
        if name:
            file_pname[name.lower()].add(aid)

TEST = ("test", "qa_", "dev", "jgibbs", "proto", "temp", "sandbox", "mockup", "gabo", "hchivers", "lmayfield", "jkoch", "ammer")
cls = collections.Counter()
with open(out_p, "w") as f:
    f.write("id\tclass\tpages\tfirst_page_names\tdisplay_names\tsame_display_in_area_with_file\tsame_page_name_in_area_with_file\n")
    for aid, row in nofile.items():
        pages = areas.get(aid, [])
        names = [n or "" for n, _, _ in pages]
        disps = sorted({d for _, d, _ in pages if d})
        same_d = sorted({a for d in disps for a in file_disp.get(d, ())})
        same_n = sorted({a for n in names for a in file_pname.get(n.lower(), ())})
        if row[1] == "phase loads it":
            c = "phase area (phs names it)"
        elif not pages and aid in areas:
            c = "map-data stub (no pages)"
        elif not pages:
            c = "mapAreasDataProto entry only"
        elif any(t in " ".join(names).lower() for t in TEST):
            c = "dev/test map pages"
        elif same_d or same_n:
            c = "pages duplicate an area that has a file"
        else:
            c = "pages unique to this area"
        cls[c] += 1
        f.write("\t".join([aid, c, str(len(pages)), ",".join(names[:5]), " | ".join(disps[:4]),
                           ",".join(same_d[:3]), ",".join(same_n[:3])]) + "\n")
print(dict(cls))
