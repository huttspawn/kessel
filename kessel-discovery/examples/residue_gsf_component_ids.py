"""Prove scFFComponentId = fold(kind minus _scs) and name the orphan cost ids."""
import json
import re
import sys

sys.path.insert(0, ".")
from residue_fnv import folds  # noqa: E402

scs = [json.loads(l) for l in open("scs.jsonl")]
ok = sum(1 for o in scs if folds(o["kind"][:-4])["xor16"] == o["v"]["4000004cdbcfe3eb"])
print("cid == fold(kind[:-4]):", ok, "of", len(scs))
cids = {o["v"]["4000004cdbcfe3eb"] for o in scs}
cost = [r["k"] for r in json.load(open("scFFComponentsCostPrototype.json"))["953b9c77"]]
up = [r["k"] for r in json.load(open("scFFComponentUpgradesCostPrototype.json"))["983fb4e0"]]
targets = {k for k in cost + up if k not in cids}
# build vocabulary from kinds
kinds = {o["kind"][:-4] for o in scs}
slots, facs, classes, tails = set(), {"imp", "rep", "neu", "premium"}, set(), set()
for k in kinds:
    m = re.match(r"(pweap|sweap|eng|shield|sys)_(imp|rep|premium)_((?:premium_)?[a-z]+(?:_[a-z]+)*?_(?:scout|striker|gunship|bomber|infiltrator)|scout|striker|gunship|bomber|infiltrator)_(.+)$", k)
    if m:
        slots.add(m.group(1))
        classes.add(m.group(3))
        tails.add(m.group(4))
    else:
        m = re.match(r"(armor|capacitor|magazine|reactor|sensor|thruster)_([a-z]+)_(.+)$", k)
        if m:
            slots.add(m.group(1))
            classes.add(m.group(2))
            tails.add(m.group(3))
classes |= {"premium_" + c for c in ["fighter_striker", "battle_scout", "artillery_gunship", "skirmisher_gunship", "drone_controller_bomber"]}
classes |= {"turret", "drone", "neu", "shuttle", "premium"}
cands = set()
for s in slots:
    for t in tails:
        for c in classes:
            cands.add(f"{s}_{c}_{t}")
            for f in facs:
                cands.add(f"{s}_{f}_{c}_{t}")
        for f in facs:
            cands.add(f"{s}_{f}_{t}")
print("candidates", len(cands))
named = {}
for n in cands:
    f = folds(n)["xor16"]
    if f in targets:
        named[f] = n
print("orphans named", len(named), "of", len(targets))
for f in sorted(targets):
    print("%016X" % f, named.get(f, "?"))
