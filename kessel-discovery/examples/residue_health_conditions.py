"""B2: list every health condition (e251d1cd enum 19) with its params, per ability."""
import collections
import json
import sys

path, pat = sys.argv[1], sys.argv[2]
CT, IP, FP = "40000004e251d1cd", "40000004e251d1d5", "40000004e251d1d6"
types = collections.Counter()


def walk(v, out):
    if isinstance(v, dict):
        if CT in v:
            types[json.dumps(v[CT])] += 1
            if v[CT] == {"enum": 19}:
                ip = {x["k"]["enum"]: x["v"] for x in v.get(IP, []) if isinstance(x.get("k"), dict)}
                fp = {x["k"]["enum"]: x["v"] for x in v.get(FP, []) if isinstance(x.get("k"), dict)}
                out.append((ip, fp))
        for x in v.values():
            walk(x, out)
    elif isinstance(v, list):
        for x in v:
            walk(x, out)


thr = collections.Counter()
for line in open(path):
    o = json.loads(line)
    out = []
    walk(o["v"], out)
    for ip, fp in out:
        thr[fp.get(86)] += 1
        if pat in o["fqn"]:
            print(o["fqn"][-70:], o["guid"], "int", ip, "float", fp)
print("health condition thresholds (param 86):", thr.most_common(25))
