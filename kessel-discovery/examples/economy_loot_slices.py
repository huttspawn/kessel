"""Second pass on loot hashes: try 'trp.' + every contiguous slice of the owner's FQN
(and of every object FQN in the owner's family), report resolution by source.
Updates dec-economy/loot_hashes.json in place."""
import os
import collections, json, sqlite3
S = os.environ["KESSEL_SCRATCH"]  # dir holding s368/, b407/, r371/, dec-effects/ and dec-economy/
D = S + "/dec-economy"
M = (1 << 64) - 1


def fnv(s):
    h = 0xCBF29CE484222325
    for b in s.upper().encode():
        h = ((h ^ b) * 0x100000001B3) & M
    return h - (1 << 64) if h >= 1 << 63 else h


lh = json.load(open(D + "/loot_hashes.json"))
H = {int(k): v for k, v in lh.items()}
owners = {o for v in H.values() for _, o in v["sources"]}
u = sqlite3.connect(f"file:{S}/s368/unfiltered.sqlite?mode=ro", uri=True)
fam = {".".join(o.split(".")[:3]) for o in owners}
pool = set(owners)
for (f,) in u.execute("select fqn from objects where kind in ('Item','Placeable','Hydra','Encounter','Npc','Spawn','Dynamic')"):
    if ".".join(f.split(".")[:3]) in fam:
        pool.add(f)
print("owners", len(owners), "slice pool", len(pool))
cands = set()
for f in pool:
    p = f.split(".")
    for i in range(len(p)):
        for j in range(i + 1, len(p) + 1):
            s = ".".join(p[i:j])
            cands.add("trp." + s)
            cands.add(s)
print("candidates", len(cands))
new = 0
for n in cands:
    h = fnv(n)
    if h in H and not H[h]["name"]:
        H[h]["name"] = ["fnv64", n]
        new += 1
print("newly resolved", new)
by = collections.Counter()
tot = collections.Counter()
for h, v in H.items():
    kinds = {s for s, _ in v["sources"]}
    for k in kinds:
        tot[k] += 1
        if v["name"]:
            by[k] += 1
print("resolved by source", {k: f"{by[k]}/{tot[k]}" for k in tot})
print("total resolved", sum(1 for v in H.values() if v["name"]), "of", len(H))
pre = collections.Counter(".".join(v["name"][1].split(".")[:2]) for v in H.values() if v["name"])
print("name families", pre.most_common(15))
un = collections.Counter(".".join(o.split(".")[:3]) for v in H.values() if not v["name"] for _, o in v["sources"])
print("unresolved owner families", un.most_common(15))
json.dump({str(k): v for k, v in H.items()}, open(D + "/loot_hashes.json", "w"), indent=0)
