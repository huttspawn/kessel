#!/usr/bin/env python3
"""Decode push 2, world: measure the bonus-mission edge (the fifth quest edge kind).

Input: refs.tsv from world2_bonus_refs.py, the story spine's edges.tsv and
isolated_han_obj_req_scr.tsv (family, kind, name, fqn), and an extract for
quest_tasks text. Reports, for the isolated quests, how many the bonus edge
(parent task 40000004dc01e9d4[].400000307860fc07 -> bonus quest) attaches,
whether the parent is itself in the spine graph, and which field the bonus
quest's own payload uses to point back (if any).

Usage: world2_bonus_edge.py <refs.tsv> <edges.tsv> <isolated.tsv> <out_dir>
"""
import collections
import sys

refs_p, edges_p, iso_p, out = sys.argv[1:5]

BONUS = ".05a2a6d3[].05a2a6d1[].05a2a6d0[].dc01e9d4[].7860fc07"
bonus_of = collections.defaultdict(set)   # bonus -> parents
children = collections.defaultdict(set)   # parent -> bonuses
src_refs = collections.defaultdict(list)  # src -> (path, tgt)
TASKOBJ = ".05a2a6d3[].05a2a6d1[].05a2a6d0[].6ef4d6a0[]{k}"
tobj = collections.defaultdict(set)       # quest -> quests linked as task objects (either end)
for line in open(refs_p):
    s, p, t, k = line.rstrip("\n").split("\t")
    if k != "Quest":
        continue
    src_refs[s].append((p, t))
    if p == TASKOBJ:
        tobj[t].add(s)
        tobj[s].add(t)
    if p == BONUS:
        bonus_of[t].add(s)
        children[s].add(t)

graph = collections.defaultdict(set)
for line in list(open(edges_p))[1:]:
    a, b, kind, _ = line.split("\t", 3)
    if kind == "touch":
        continue
    graph[a].add(b)
    graph[b].add(a)

iso = [l.rstrip("\n").split("\t") for l in open(iso_p)]
c = collections.Counter()
rows = []
for fam, _, nm, q in iso:
    b = "bonus" in q
    par = bonus_of.get(q, set())
    # Climb bonus -> parent until a quest with a spine edge (bonus of a bonus).
    seen, frontier, anchored = set(), set(par), None
    while frontier and anchored is None:
        nxt = set()
        for p in frontier:
            if p in seen:
                continue
            seen.add(p)
            if graph.get(p):
                anchored = p
                break
            nxt |= bonus_of.get(p, set())
        frontier = nxt
    own = sorted({p for p, _ in src_refs.get(q, [])})
    c[("bonus" if b else "other", "has_parent" if par else "no_parent")] += 1
    if not par and tobj.get(q):
        c[("bonus" if b else "other", "no_parent_but_task_object_quest")] += 1
    if par:
        c[("bonus" if b else "other", "anchored" if anchored else "parent_isolated")] += 1
    rows.append((fam, q, nm, ",".join(sorted(par)), anchored or "", ";".join(own)))

with open(f"{out}/bonus_edge.tsv", "w") as f:
    f.write("family\tquest\tname\tparents\tanchored_via\town_quest_ref_paths\n")
    for r in rows:
        f.write("\t".join(r) + "\n")
print("bonus edges", sum(len(v) for v in children.values()),
      "parents", len(children), "bonuses", len(bonus_of))
for k, v in sorted(c.items()):
    print(k, v)
