"""Catalog every script-raised achievement event with its root triggers (#368, FR-KESSEL-025 c7).

usage: python3 -I hidden_earnables.py <dir>
<dir> holds Hydra/Conversation/Ability/Quest/Achievement.jsonl (dump_kind_jsonl on an unfiltered
extract), lgcach.json (dump_singleton <db> lgcAchievementEventsPrototype) and optionally
s368/sel_names.json ({signed fnv: selector name}). Writes <dir>/s368/hidden_earnables.tsv:
script, handler, event name, event code, who is credited, root triggers (On Custom handlers
followed back through Execute Hydra Script calls and quest/achievement script hooks),
achievements counting the event, and their visibility.
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
ev = {e["k"] & M: [str(a).lstrip("@") for a in e["v"]] for e in lgc["55f64de0"] if isinstance(e["k"], int)}
vis = {}
for line in open(f"{S}/Achievement.jsonl"):
    o = json.loads(line)
    vis[o["fqn"]] = ["always", "progress", "completion", "conditional"][o["v"].get("e866db30", {"enum": 0})["enum"]]

# per-handler action lists
raises = []  # (script, handler, name, selector)
calls = collections.defaultdict(set)  # "script:Handler" lowercased -> {(caller, caller_handler)}
SEL = {}


def selname(h):
    return SEL.get(h, str(h))


HOOK = {"e848b9bb": "quest step hook", "cdf0591a": "quest step hook", "cdf05962": "quest fail hook",
        "daed3b0a": "quest increment hook", "e4c167cc": "quest hook", "e4c167cd": "quest hook",
        "debd1140": "quest hook", "39fad2f0": "quest hook", "c76ea755": "achievement complete hook"}


def collect(v, fqn, handler, out, key=None):
    if isinstance(v, dict):
        a = v.get("e5346fa0")
        if a == "Achievement Event":
            out.append(("raise", fqn, handler, v.get("e5346faf"), v.get("ed10e6a4")))
        elif a == "Execute Hydra Script" and isinstance(v.get("e5346faf"), str):
            out.append(("call", fqn, handler, v["e5346faf"], None))
        if isinstance(v.get("4fa14a1f"), str) and isinstance(v.get("cdeb4375"), str):
            out.append(("call", fqn, HOOK.get(key, f"hook {key}"), f"{v['4fa14a1f'].lstrip('@')}:{v['cdeb4375']}", None))
        for k, x in v.items():
            collect(x, fqn, handler, out, k)
    elif isinstance(v, list):
        for x in v:
            if isinstance(x, dict) and set(x) == {"k", "v"} and isinstance(x["k"], str) and x["k"].startswith("On "):
                collect(x["v"], fqn, x["k"], out, key)
            else:
                collect(x, fqn, handler, out, key)


for kind in ("Hydra", "Conversation", "Ability", "Quest", "Achievement"):
    for line in open(f"{S}/{kind}.jsonl"):
        o = json.loads(line)
        out = []
        collect(o["v"], o["fqn"], kind.lower() if kind != "Hydra" else None, out)
        for t, fqn, h, val, sel in out:
            if t == "raise":
                raises.append((fqn, h, val, sel))
            else:
                calls[val.lower()].add((fqn, h or "?"))

try:
    SEL.update({int(k): v for k, v in json.load(open(f"{S}/s368/sel_names.json")).items()})
except FileNotFoundError:
    pass


def roots(script, handler, depth=0, seen=None):
    """Follow On Custom handlers back through Execute Hydra Script callers."""
    seen = seen or set()
    if not handler or not handler.startswith("On Custom:") or depth > 4:
        return {f"{script} {handler}"}
    key = f"{script}:{handler.split(':', 1)[1]}".lower()
    if key in seen:
        return set()
    seen.add(key)
    callers = calls.get(key, set())
    if not callers:
        return {f"{script} {handler} (no caller found)"}
    r = set()
    for c, ch in callers:
        r |= roots(c, ch, depth + 1, seen)
    return r


rows = []
stat = collections.Counter()
for script, handler, name, sel in raises:
    c = code(name) if name else None
    achs = ev.get(c, [])
    rs = sorted(roots(script, handler))
    stat["actions"] += 1
    stat["resolved" if achs else "no_achievement"] += 1
    if any("(no caller found)" in r for r in rs):
        stat["root_unfound"] += 1
    rows.append([script, handler or "", name, f"{c:016X}" if c else "", selname(sel), " | ".join(rs[:6]),
                 " ".join(achs), " ".join(sorted({vis.get(a, '?') for a in achs}))])
with open(f"{S}/s368/hidden_earnables.tsv", "w") as f:
    f.write("script\thandler\tevent_name\tevent_code\tcredited\troot_triggers\tachievements\tvisibility\n")
    for r in rows:
        f.write("\t".join(r) + "\n")
print(stat)
print("distinct scripts", len({r[0] for r in rows}), "names", len({r[2] for r in rows}),
      "achievements", len({a for r in rows for a in r[6].split()}))
rt = collections.Counter(r.split(" ", 1)[1].split(":")[0] if " " in r else r for row in rows for r in row[5].split(" | "))
print(rt.most_common(20))
print(collections.Counter(r[4] for r in rows).most_common(10))
