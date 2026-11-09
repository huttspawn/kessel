"""Decode the taxi graph: tax nodes, routes (cost, destination, path, reversed),
faction hash, terminal NPCs and script unlocks. Writes dec-economy/taxis.json."""
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


tax = {}
for l in open(D + "/kind/tax.jsonl"):
    j = json.loads(l)
    tax[j["fqn"]] = j["v"]
edges = []
for f, v in tax.items():
    for r in v.get("b981a0b3", []):
        edges.append((f, r.get("b981a0af"), r.get("b981a0ad"), r.get("b981a0b1"), bool(r.get("b981a0b0"))))
res = sum(1 for e in edges if isinstance(e[1], str) and e[1][1:] in tax)
print("tax nodes", len(tax), "with routes", sum(1 for v in tax.values() if v.get("b981a0b3")), "edges", len(edges), "dest resolves", res)
print("edges with cost", sum(1 for e in edges if e[2] is not None), "cost range", min(e[2] for e in edges if e[2]), max(e[2] for e in edges if e[2]))
# symmetric?
es = {(a, b[1:]) for a, b, *_ in edges if isinstance(b, str)}
print("edges whose reverse exists", sum(1 for a, b in es if (b, a) in es), "of", len(es))
fac = collections.Counter(v.get("7ab2caa6") for v in tax.values())
names = {fnv(n): n for n in ["REPUBLIC", "EMPIRE", "NEUTRAL", "BOTH", "ALL", "NONE", "IMPERIAL", "faction_republic", "faction_empire", "republic", "empire", "neutral", "fac.republic", "fac.empire", "fac.neutral", "Republic", "Empire", "Neutral", "fac_republic", "fac_empire", "fac_neutral", "0"]}
print("factions", [(k, names.get(k), n) for k, n in fac.most_common()])
# path objects
u = sqlite3.connect(f"file:{S}/s368/unfiltered.sqlite?mode=ro", uri=True)
pth = {r[0] for r in u.execute("select fqn from objects where fqn like 'pth.%'")}
print("route paths resolving to pth objects", sum(1 for e in edges if e[3] in pth), "of", len(edges))
# names
strs = dict(u.execute("select id2, text from strings where fqn like 'str.tax.%' or fqn like 'str.npc.%'"))
print("taxNameId samples", [(f, v.get("f75aa580")) for f, v in list(tax.items())[:3]])
# terminal npcs
term = []
for l in open(D + "/kind/Npc.jsonl"):
    if "de992c09" in l:
        j = json.loads(l)
        t = j["v"].get("de992c09")
        if t is not None:
            term.append((j["fqn"], t))
print("terminal npcs", len(term), "spec resolves to tax", sum(1 for _, t in term if isinstance(t, str) and t[1:] in tax), term[:3])
# script unlocks
b = sqlite3.connect(f"file:{S}/b407/spice.sqlite?mode=ro", uri=True)
un = b.execute("select action, script_fqn, handler, target_fqn from script_actions where action in ('Unlock Taxi Destination','Lock Taxi Destination','Take Taxi Route','Queue Taxi Route','Show Taxi UI')").fetchall()
c = collections.Counter(a for a, *_ in un)
print("script taxi actions", dict(c))
ut = [t for a, _, _, t in un if a == "Unlock Taxi Destination"]
print("unlock targets resolve to tax", sum(1 for t in ut if t in tax), "of", len(ut), "distinct", len(set(ut)))
roots = collections.Counter(h.split(":")[0] for a, _, h, _ in un if a == "Unlock Taxi Destination")
print("unlock handlers", roots.most_common(6))
tr = [t for a, _, _, t in un if a in ("Take Taxi Route", "Queue Taxi Route")]
print("route actions targets sample", tr[:3])
json.dump({"nodes": tax, "edges": edges, "terminals": term, "unlocks": un}, open(D + "/taxis.json", "w"))
