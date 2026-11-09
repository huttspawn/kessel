"""Name low-32 field ids from client.gom names and the field registry.
usage: names.py <low32hex>..."""
import json, sys
W = "docs/fields/"
names = {}
for r in json.load(open(W + "names.json"))["records"]:
    names.setdefault(r["id_hex"][-8:].lower(), []).append(r["name"])
reg = {}
for l in open(W + "registry.jsonl"):
    j = json.loads(l)
    reg[j["id"][-8:].lower()] = j
for a in sys.argv[1:]:
    a = a.lower()
    j = reg.get(a, {})
    print(a, names.get(a), j.get("id"), j.get("status"), (j.get("meaning") or "")[:100])
