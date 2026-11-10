#!/usr/bin/env python3
"""Decode push 3, world: final classes for the areas the map data names with no area file.

Reads pages.tsv (world3_mapdata_pages.py), nofile_class.tsv (world3_area_nofile.py) and the
archive checks for live and 8.0 PTS (world3_dir_files.rs). Rules, first match wins:
  placeholder ids   mapAreasDataProto keys 50/51 (unk_chat, off_chat): not area ids.
  phase area        a phs names it as the area it loads (24ed04cd).
  no map content    mapdata object has no pages, or only a mapAreasDataProto entry.
  exp.01 class arc  page names of the cut first-expansion class story: Bothawui (map_bot_*,
                    bothawui_*), its class hubs (hub1_<class>), and the per-class instanced
                    'Placeholder' maps (map_<bh|sm|tr|jk|jc|ia|si|sw>_*).
  greybox / dev     developer names, greybox, test, generic map1/world_map pages with no or
                    throwaway display names.
  superseded        a display name equal to one of an area that has a file.
  other             the rest, listed for reading.
Prints counts and writes final.tsv.

Usage: world3_area_nofile_final.py <pages.tsv> <nofile_class.tsv> <dirs_live.tsv> <dirs_pts.tsv> <out.tsv>
"""
import re
import sys

pages_p, cls_p, live_p, pts_p, out_p = sys.argv[1:6]
pages = {l.split("\t")[0]: l.rstrip("\n").split("\t") for l in list(open(pages_p))[1:]}
base = {l.split("\t")[0]: l.rstrip("\n").split("\t") for l in list(open(cls_p))[1:]}
live = {l.split("\t")[0]: l.rstrip("\n").split("\t") for l in list(open(live_p))[1:]}
pts = {l.split("\t")[0]: l.rstrip("\n").split("\t") for l in list(open(pts_p))[1:]}

CLASS_ARC = re.compile(r"(^|,)(map_bot_|bothawui|hub1_(consular|knight|smuggler|trooper|agent|hunter|inquisitor|warrior)|map_(bh|sm|tr|jk|jc|ia|si|sw)_)")
DEV = re.compile(r"(jgibbs|kquinn|jcates|brebouche|mhadley|william|edwin|martin|bobby|greybox|greaybox|playground|sniff|test|kittens|awesomeville|baadgai|qist|winstonia|map22211|meh$|nate)", re.I)
GENERIC = {"world_map", "map1", "map2", "map3", "main_map", "map_world", "small_map", "another1", "interior", "surface", "map_mainarea", "area_map", "map_maze", "valley", "map_int", "map_house1", "interior_map_office", "map_bot_room_1", "inner_map", "hub_map", "region_one", "region_two", "region_three", "region_four", "cubes", "tomb2", "korrwarr_greybox"}

# Read by hand from their page and display names (the residue the rules leave).
HAND = {
    "4611686034513170078": "developer greybox / test map",   # Taxi House 1/2, Taxi M20S1
    "4611686062042431568": "developer greybox / test map",   # Taxi Room
    "4611686034963370246": "developer greybox / test map",   # Space Area
    "4611686061171031295": "developer greybox / test map",   # map_prototype_raid_50
    "4611686045576570000": "developer greybox / test map",   # map_pvp_world Imperial/Republic Side
    "4611686045100172279": "removed area (shipped once)",    # Ord Mantell Station, kgt_newintel; 7 stale dictionary paths
    "4611686283890757631": "cut exp.01 class story (Bothawui / class instances)",  # Pale Omen per-class hangars
    "4611686283890770463": "cut exp.01 class story (Bothawui / class instances)",  # The Resolute per-class hangars
}
counts = {}
rows = []
for aid in sorted(base, key=int):
    b = base[aid]
    p = (pages.get(aid, [aid]) + [""] * 7)[:7]
    names = [n for n in p[3].split(",") if n]
    disp = p[4]
    if aid in ("50", "51"):
        c = "placeholder ids (not area ids)"
    elif b[1] == "phase loads it":
        c = "phase area (phs loads it)"
    elif p[1] in ("map-data stub (no pages)", "mapAreasDataProto entry only"):
        c = "no map content (stub)"
    elif CLASS_ARC.search(p[3]) or "Bothawui" in disp:
        c = "cut exp.01 class story (Bothawui / class instances)"
    elif DEV.search(p[3] + " " + disp) or (names and all(n in GENERIC for n in names)):
        c = "developer greybox / test map"
    elif aid in HAND:
        c = HAND[aid]
    elif p[5]:
        c = "superseded copy of a shipped area"
    else:
        c = "other"
    counts[c] = counts.get(c, 0) + 1
    rows.append([aid, c, p[2], p[3][:120], disp[:120], p[5], live[aid][3], pts[aid][1]])
with open(out_p, "w") as f:
    f.write("id\tclass\tpages\tpage_names\tdisplay_names\tsame_display_with_file\tdict_paths_live\tarea_dat_on_pts\n")
    for r in rows:
        f.write("\t".join(r) + "\n")
for k, v in sorted(counts.items(), key=lambda kv: -kv[1]):
    print(v, k)
