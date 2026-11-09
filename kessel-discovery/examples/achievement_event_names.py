"""Classify every lgcAchievementEvents key by the name that hashes to it (#368).

An event code is 0xE000<<48 | fold48(FNV-1a 64 of the uppercased name). Names tried: every
object and singleton fqn (fqns.tsv: fqn TAB kind), every Hydra 'Achievement Event' name
(achev_rows.json) and every short string in the decoded objects.
usage: python3 -I achievement_event_names.py <dir>  (same <dir> as hidden_earnables.py)
"""
import json, sys, collections

S = sys.argv[1]
M = (1 << 64) - 1


def fnv(s):
    h = 0xcbf29ce484222325
    for b in s.encode():
        h ^= b
        h = (h * 0x100000001b3) & M
    return h


def code(n):
    h = fnv(n.upper())
    return 0xE000000000000000 | (h & 0x0000FFFFFFFF0000) | ((h ^ (h >> 48)) & 0xFFFF)


lgc = json.load(open(f"{S}/lgcach.json"))
ev = {}
for e in lgc["55f64de0"]:
    k = e["k"]
    if isinstance(k, int) and (k & M) >> 48 == 0xE000:
        ev[k & M] = e["v"]
small = [e["k"] for e in lgc["55f64de0"] if isinstance(e["k"], int) and (e["k"] & M) >> 48 != 0xE000]
print("E000 event keys", len(ev), "other keys", len(small), small[:10])

names = {}  # code -> (source, name)
for line in open(f"{S}/fqns.tsv"):
    p = line.rstrip("\n").split("\t")
    fqn = p[0]
    kind = p[1] if len(p) > 1 else "singleton"
    c = code(fqn)
    if c in ev:
        names.setdefault(c, (f"fqn:{kind}", fqn))
try:
    rows = json.load(open(f"{S}/achev_rows.json"))  # optional: [kind, script, handler, name]
except FileNotFoundError:
    rows = []  # script names are also caught by the string pass below
for kind, fqn, handler, name, *_ in rows:
    if name and code(name) in ev:
        names.setdefault(code(name), ("script", name))
# all strings anywhere in hydra/quest/conversation/achievement
strs = collections.Counter()


def walk(v):
    if isinstance(v, dict):
        for x in v.values():
            walk(x)
    elif isinstance(v, list):
        for x in v:
            walk(x)
    elif isinstance(v, str) and len(v) < 120:
        strs[v] += 1


for k in ("Hydra", "Quest", "Conversation", "Achievement"):
    for line in open(f"{S}/{k}.jsonl"):
        walk(json.loads(line)["v"])
for s in strs:
    for t in (s, s.split(":")[-1]):
        if code(t) in ev:
            names.setdefault(code(t), ("string", t))

print("resolved", len(names), "of", len(ev))
src = collections.Counter(v[0] for v in names.values())
print(src)
pref = collections.Counter(v[1].split(".")[0] for v in names.values() if v[0].startswith("fqn"))
print(pref.most_common(20))
unres = [k for k in ev if k not in names]
json.dump({f"{k:016X}": v for k, v in names.items()}, open(f"{S}/event_names.json", "w"))
json.dump([f"{k:016X}" for k in unres], open(f"{S}/event_unresolved.json", "w"))
for k in unres[:15]:
    print(f"{k:016X}", ev[k][:3])
