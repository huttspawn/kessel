"""Decorations: dec -> item -> every known source (item_source label, quest reward,
achievement reward, collection entry, script give, loot package owner, vendor
category label, reputation requirement). Writes dec-economy/dec_sources.json."""
import os
import collections, json, sqlite3
S = os.environ["KESSEL_SCRATCH"]  # dir holding s368/, b407/, r371/, dec-effects/ and dec-economy/
D = S + "/dec-economy"
dec = [json.loads(l) for l in open(D + "/kind/dec.jsonl")]
items = {}
for l in open(D + "/kind/Item.jsonl"):
    if '"itm.stronghold' in l or "dec" in l or "9ef1a7fa" in l or "f9b95e90" in l:
        j = json.loads(l)
        items["@" + j["fqn"]] = j["v"]
labels = json.load(open(S + "/r371/itmsource.json.resolved.json"))
rw = json.load(open(S + "/rewards.json"))
qitems = set()
for q in rw["e146d0c0"]:
    for r in q["v"]:
        qitems.add(r.get("d9f35ee6"))
for q in rw["c788046a"]:
    for b in q["v"]:
        for r in b["v"]:
            qitems.add(r.get("d9f35ee6"))
ach = json.load(open(S + "/achRewardsTable_Prototype.json"))
aitems = set()
for row in ach["0f72cd27"]:
    for it in row["v"].get("0f72cd25", []):
        aitems.add(it.get("0f72cd22"))
col = json.load(open(D + "/collections.json"))
citems = {i for e in col["entries"].values() for i in e["items"]}
cent = set(col["entries"])
b = sqlite3.connect(f"file:{S}/b407/spice.sqlite?mode=ro", uri=True)
gitems = {"@" + r[0] for r in b.execute("select target_fqn from script_actions where action in ('Give Items','Give Quest Item') and target_fqn is not null")}
cnt = collections.Counter()
lab = collections.Counter()
out = {}
for d in dec:
    v = d["v"]
    it = v.get("7671280b")
    iv = items.get(it)
    src = []
    if iv is None:
        cnt["item missing"] += 1
    else:
        cnt["item resolves"] += 1
        sl = iv.get("9ef1a7fa")
        sl = sl if isinstance(sl, list) else ([sl] if sl is not None else [])
        names = [labels.get(str(s)) for s in sl]
        if names:
            src.append(("label", names))
            for n in names:
                lab[(n or "?").split(":")[0]] += 1
        if iv.get("f9b95e90") is not None:
            src.append(("requires reputation", [iv.get("f9b95e90"), iv.get("f9b95e91")]))
    if it in qitems:
        src.append(("quest reward", []))
    if it in aitems:
        src.append(("achievement reward", []))
    if it in citems or str(v.get("0b8597f0")) in cent:
        src.append(("collection", []))
    if it in gitems:
        src.append(("script give", []))
    if v.get("0b8597f0") is not None:
        cnt["decMtxId"] += 1
        if str(v.get("0b8597f0")) in cent:
            cnt["decMtxId -> collection entry"] += 1
    for k, _ in src:
        cnt["src " + k] += 1
    if not src:
        cnt["no source"] += 1
    out[d["fqn"]] = {"item": it, "sources": src, "server_npc": v.get("36553800"), "guild_cost": v.get("ad770098")}
print(dict(cnt))
print("label families", lab.most_common(15))
json.dump(out, open(D + "/dec_sources.json", "w"))
