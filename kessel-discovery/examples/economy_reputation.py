"""Reputation sources: GrantReputation effects (from dec-effects/effects.jsonl),
item -> ability -> (group, points), itmReputationTrophyGroup agreement, quest
reward rows that hand out trophies, and reputation-gated items (vendor stock proxy).
Writes dec-economy/reputation.json."""
import os
import collections, json
S = os.environ["KESSEL_SCRATCH"]  # dir holding s368/, b407/, r371/, dec-effects/ and dec-economy/
D = S + "/dec-economy"
groups = {r["k"]: r["v"] for r in json.load(open(D + "/sg/repGroupInfosPrototype.json"))["f83f8fd0"]}
grant = collections.defaultdict(list)  # ability fqn -> [(group, points)]
other_ops = collections.Counter()
for l in open(S + "/dec-effects/effects.jsonl"):
    if "Reputation" not in l:
        continue
    j = json.loads(l)
    for b in j.get("blocks", []):
        for a in b.get("act", []):
            if a["op"] == "GrantReputation":
                p = a["params"]
                grant[j["fqn"]].append((p.get("ReputationGroupId"), p.get("ReputationPoints")))
            elif "eput" in a["op"]:
                other_ops[a["op"]] += 1
print("abilities with GrantReputation", len(grant), "other rep ops", dict(other_ops))
fam = collections.Counter(f.split(".")[1] for f in grant)
print("ability families", fam.most_common(10))
gid = collections.Counter(g for v in grant.values() for g, _ in v)
print("groups granted", len(gid), "all in repGroupInfos", all(g in groups for g in gid))
items = {}
for l in open(D + "/kind/Item.jsonl"):
    if "2d7b8779" in l and ("reputation" in l or "98709f20" in l):
        j = json.loads(l)
        items[j["fqn"]] = j["v"]
it_rep = {}
agree = disagree = 0
for f, v in items.items():
    ab = v.get("2d7b8779")
    if isinstance(ab, str) and ab[1:] in grant:
        it_rep[f] = {"ability": ab[1:], "grants": grant[ab[1:]], "trophy_group": v.get("98709f20")}
        tg = v.get("98709f20")
        if tg is not None:
            if any(g == tg for g, _ in grant[ab[1:]]):
                agree += 1
            else:
                disagree += 1
print("items whose use ability grants reputation", len(it_rep), "trophy group agrees", agree, "disagrees", disagree)
pts = collections.Counter(p for v in it_rep.values() for _, p in v["grants"])
print("points values", pts.most_common(12))
rw = json.load(open(S + "/rewards.json"))
qrows = collections.Counter()
for q in rw["e146d0c0"]:
    for r in q["v"]:
        it = r.get("d9f35ee6")
        if isinstance(it, str) and it[1:] in it_rep:
            qrows[q["k"]] += 1
print("quests rewarding a reputation item", len(qrows), "rows", sum(qrows.values()))
# gated items
gated = collections.Counter()
for l in open(D + "/kind/Item.jsonl"):
    if "f9b95e90" in l:
        j = json.loads(l)
        req = j["v"].get("fe1c7a31")  # itmRequiredReputation {group f9b95e90, rank f9b95e91}
        if isinstance(req, dict) and req.get("f9b95e90") is not None:
            gated[(req["f9b95e90"], req.get("f9b95e91"))] += 1
print("reputation-gated items", sum(gated.values()), "groups", len({g for g, _ in gated}), "ranks", collections.Counter(r for _, r in gated.elements()).most_common())
gated = {f"{g}:{r}": n for (g, r), n in gated.items()}
out = {"groups": {k: {"name": v.get("b0a72730"), "weekly_cap": v.get("07182120")} for k, v in groups.items()},
       "ability_grants": grant, "item_grants": it_rep, "quests": qrows, "gated_items_by_group": gated}
json.dump(out, open(D + "/reputation.json", "w"), default=list)
