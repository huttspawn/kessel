"""Search every dumped singleton and object kind for the loot-package hashes and for
vendor-package / mail names outside their known owner fields. Any hit outside the
owner fields would be a client-side table for them."""
import os
import json, os, re
D = os.environ["KESSEL_SCRATCH"] + "/dec-economy"  # dumps from dump_kind_jsonl, dump_singleton, dump_stb
H = {int(k) for k in json.load(open(D + "/loot_hashes.json"))}
H |= {h + (1 << 64) for h in H if h < 0}
OWN = {"407c89a3", "ae2f555b"}
hits = {}
strhits = {}


def walk(v, path, src):
    if isinstance(v, dict):
        for k, x in v.items():
            walk(x, path + "/" + k, src)
    elif isinstance(v, list):
        for x in v:
            walk(x, path, src)
    elif isinstance(v, bool):
        return
    elif isinstance(v, int):
        if v in H and path.split("/")[-1] not in OWN:
            hits.setdefault((src, path), 0)
            hits[(src, path)] += 1
    elif isinstance(v, str):
        if v.startswith(("pkg.vendor", "mail.", "trp.", "@trp.", "@mail.", "@pkg.vendor")):
            strhits.setdefault((src, path), 0)
            strhits[(src, path)] += 1


for f in sorted(os.listdir(D + "/sg")):
    if f.endswith(".json") and os.path.getsize(D + "/sg/" + f) > 2:
        try:
            walk(json.load(open(D + "/sg/" + f)), "", "sg:" + f[:-5])
        except json.JSONDecodeError:
            pass
for f in sorted(os.listdir(D + "/kind")):
    if f.endswith(".jsonl"):
        for l in open(D + "/kind/" + f):
            j = json.loads(l)
            walk(j["v"], "", "kind:" + f[:-6])
print("loot-hash hits outside owner fields:")
for k, n in sorted(hits.items(), key=lambda x: -x[1])[:40]:
    print(" ", n, k)
print("vendor/mail/trp name hits:")
for k, n in sorted(strhits.items(), key=lambda x: -x[1])[:40]:
    print(" ", n, k)
