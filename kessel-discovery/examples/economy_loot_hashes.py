"""Collect every loot-package hash the client names, and try to resolve them by
FNV-1a 64 (and the 48-bit GUID fold) of uppercased candidate names.
Writes dec-economy/loot_hashes.json."""
import os
import collections, json, re, sqlite3
S = os.environ["KESSEL_SCRATCH"]  # dir holding s368/, b407/, r371/, dec-effects/ and dec-economy/
D = S + "/dec-economy"
M64 = (1 << 64) - 1


def fnv(s):
    h = 0xCBF29CE484222325
    for b in s.upper().encode():
        h ^= b
        h = (h * 0x100000001B3) & M64
    return h


def fold(h):
    return 0xE000000000000000 | (h & 0x0000FFFFFFFF0000) | ((h ^ (h >> 48)) & 0xFFFF)


def s64(u):
    return u - (1 << 64) if u >= 1 << 63 else u


hashes = collections.defaultdict(set)  # signed i64 -> set of (source, owner fqn)
for l in open(D + "/kind/Item.jsonl"):
    j = json.loads(l)
    v = j["v"]
    if isinstance(v, dict) and "407c89a3" in v and isinstance(v["407c89a3"], int):
        hashes[v["407c89a3"]].add(("item 407c89a3", j["fqn"]))
for l in open(D + "/kind/Placeable.jsonl"):
    j = json.loads(l)
    v = j["v"]
    if isinstance(v, dict) and "ae2f555b" in v:
        hashes[v["ae2f555b"]].add(("plc ae2f555b", j["fqn"]))
c = sqlite3.connect(f"file:{S}/b407/spice.sqlite?mode=ro", uri=True)
for fqn, val in c.execute("select script_fqn, value from script_actions where action='Roll Loot Package'"):
    try:
        hashes[int(val)].add(("script Roll Loot Package", fqn))
    except ValueError:
        pass
print("distinct loot hashes", len(hashes), collections.Counter(s for v in hashes.values() for s, _ in v))

# candidate names
cands = set()
u = sqlite3.connect(f"file:{S}/s368/unfiltered.sqlite?mode=ro", uri=True)
fqns = [r[0] for r in u.execute("select fqn from objects")]
for f in fqns:
    cands.add(f)
    parts = f.split(".")
    for i in range(1, len(parts)):
        tail = ".".join(parts[i:])
        cands.add("trp." + tail)
        cands.add("trp." + f)
    cands.add("trp_" + f)
# strings in the strings table and in scripts that look like identifiers
for (t,) in c.execute("select distinct value from script_actions where value is not null"):
    if t and re.fullmatch(r"[A-Za-z0-9_.:]+", t):
        cands.add(t)
        cands.add("trp." + t)
print("candidates", len(cands))
H = set(hashes)
hit = {}
for n in cands:
    h = fnv(n)
    for form, val in (("fnv64", s64(h)), ("fold48", s64(fold(h)))):
        if val in H:
            hit[val] = (form, n)
print("resolved", len(hit), "of", len(H))
for val, (form, n) in list(hit.items())[:40]:
    print(val, form, n, sorted(hashes[val])[:2])
out = {str(k): {"sources": sorted(map(list, v)), "name": hit.get(k)} for k, v in hashes.items()}
json.dump(out, open(D + "/loot_hashes.json", "w"), indent=0)
