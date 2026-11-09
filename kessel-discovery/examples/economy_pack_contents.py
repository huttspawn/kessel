"""Cartel pack contents: collection source (pack) name == pack item name, so
pack item -> source -> collection entries -> items. Prints the join."""
import os
import collections, json, sqlite3
S = os.environ["KESSEL_SCRATCH"]  # dir holding s368/, b407/, r371/, dec-effects/ and dec-economy/
D = S + "/dec-economy"
col = json.load(open(D + "/collections.json"))
u = sqlite3.connect(f"file:{S}/s368/unfiltered.sqlite?mode=ro", uri=True)
name = collections.defaultdict(list)
for f, t in u.execute("select o.fqn, s.text from objects o join strings s on s.id2 = o.string_id and s.id1 = 0 and s.fqn like 'str.itm.%' where o.kind='Item' and (o.fqn like 'itm.mtx.%' or o.fqn like 'itm.loot.%')"):
    name[t.strip().lower()].append(f)
lh = json.load(open(D + "/loot_hashes.json"))
pkg_of = {o: int(h) for h, v in lh.items() for s, o in v["sources"] if s == "item 407c89a3"}
hit = 0
for s in col["sources"]:
    n = (s["name"] or "").strip().lower()
    fq = name.get(n, [])
    with_pkg = [f for f in fq if f in pkg_of]
    if fq:
        hit += 1
    print(f"{s['name']!r:45} entries={len(s['entries']):4} items={fq[:2]} pkg={[pkg_of[f] for f in with_pkg][:2]}")
print("sources whose name matches an itm.mtx/itm.loot item name:", hit, "of", len(col["sources"]))
