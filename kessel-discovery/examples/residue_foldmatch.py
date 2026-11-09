"""Generic fold matcher: name unresolved E000 GUIDs by folding candidate FQNs.

usage: foldmatch.py <targets.txt (one hex/decimal per line)> <jsonl or txt files...>
Candidates: every string value in the files, every objects.fqn-like string,
plus first-segment swaps onto prefixes with no loaded object (pkg wpn cbt ...).
"""
import json
import re
import sys

sys.path.insert(0, ".")
from residue_fnv import folds  # noqa: E402

PREFIXES = ["pkg", "wpn", "cbt", "spec", "abl", "npc", "itm", "class", "apn", "npp", "med", "sce", "veh",
            "app", "ipp", "epp", "skl", "stat", "mod", "anim", "ani", "snd", "fx"]


def load_targets(p):
    out = set()
    for line in open(p):
        s = line.strip()
        if not s:
            continue
        out.add(int(s, 16) if re.fullmatch(r"[0-9A-Fa-f]{16}", s) else int(s) & 0xFFFFFFFFFFFFFFFF)
    return out


def strings(v, acc):
    if isinstance(v, str):
        acc.add(v.lstrip("@"))
    elif isinstance(v, dict):
        for k, x in v.items():
            strings(x, acc)
    elif isinstance(v, list):
        for x in v:
            strings(x, acc)


def main():
    targets = load_targets(sys.argv[1])
    acc = set()
    for f in sys.argv[2:]:
        for line in open(f):
            line = line.strip()
            if not line:
                continue
            try:
                strings(json.loads(line), acc)
            except json.JSONDecodeError:
                acc.add(line)
    cands = set(acc)
    for s in acc:
        seg = s.split(".")
        if len(seg) >= 2 and re.fullmatch(r"[A-Za-z0-9_.]+", s):
            for p in PREFIXES:
                cands.add(p + "." + ".".join(seg[1:]))
                cands.add(p + "." + s)
            for i in range(2, len(seg)):
                cands.add(".".join(seg[:i]))
    hits = {}
    for c in cands:
        f = folds(c)["xor16"]
        if f in targets:
            hits[f] = c
    print("candidates", len(cands), "named", len(hits), "of", len(targets))
    for t in sorted(targets):
        print("%016X" % t, hits.get(t, "?"))


main()
