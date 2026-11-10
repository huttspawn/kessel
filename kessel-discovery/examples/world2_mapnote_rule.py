#!/usr/bin/env python3
"""Decode push 2, world: the map-note show and hide rule.

Joins three sources:
  - mpn templates (kind 'Phase' in kessel, dump_kind_jsonl Phase.jsonl): type
    e986726f, mpnVisibilityCondition 9b7e5300, mpnDoNotDisplay d852e845,
    mpnDoNotDisplayOnMinimap e7e45c30;
  - placed notes (mapnotes.jsonl from world_mapnotes.py): template, flags;
  - quest payloads (qst.jsonl): per task qstTaskMapNoteList bce1f1c6, and the
    map-note quest variables (qstMapNoteVariableDefinition_ProtoVarList
    ad640056, asset c31b8e30, id b21e56a4) that a step adds (c8b89fb2) and a
    step or task removes (c8b89fb3).

Reports, per template type, how each placed note can be shown: by an active
task's list, by a quest map-note variable, statically, or never.

Usage: world2_mapnote_rule.py <Phase.jsonl> <mapnotes.jsonl> <qst.jsonl> <out_dir>
"""
import collections
import json
import sys

tpl_p, notes_p, qst_p, out = sys.argv[1:5]

tpl = {}
for line in open(tpl_p):
    r = json.loads(line)
    v = r["v"]
    tpl[r["fqn"]] = {
        "type": (v.get("e986726f") or {}).get("enum"),
        "cond": v.get("9b7e5300"),
        "dnd": bool(v.get("d852e845")),
        "dnd_mini": bool(v.get("e7e45c30")),
        "icon": v.get("7946402a"),
    }

task_listed = collections.Counter()      # template -> number of tasks listing it
var_listed = collections.Counter()       # template -> number of map-note variables naming it
var_life = collections.Counter()         # (add step -> remove step) gap
var_where = collections.Counter()
multi_task_steps = collections.Counter()
same_across_steps = collections.Counter()
nonmpn = collections.Counter()

for line in open(qst_p):
    q = json.loads(line)
    v = q["v"]
    mvars = {}
    for d in v.get("ad640056", []) or []:
        if isinstance(d, dict) and "b21e56a4" in d:
            mvars[d["b21e56a4"]] = d.get("c31b8e30")
            if d.get("c31b8e30"):
                var_listed[d["c31b8e30"]] += 1
    for br_i, br in enumerate(v.get("05a2a6d3", []) or []):
        added = {}
        prev = None
        for st_i, st in enumerate(br.get("05a2a6d1", []) or []):
            for x in st.get("c8b89fb2", []) or []:
                if x in mvars:
                    added[x] = st_i
            for x in st.get("c8b89fb3", []) or []:
                if x in mvars:
                    if x in added:
                        var_life[min(st_i - added.pop(x), 5)] += 1
                    else:
                        var_where["removed_without_add"] += 1
            tasks = st.get("05a2a6d0", []) or []
            lists = []
            for t in tasks:
                ml = t.get("bce1f1c6", []) or []
                for m in ml:
                    if m.startswith("mpn."):
                        task_listed[m] += 1
                    else:
                        nonmpn[m.split(".", 1)[0]] += 1
                for x in t.get("c8b89fb3", []) or []:
                    if x in mvars and x in added:
                        var_life["task_removes"] += 1
                        added.pop(x)
                lists.append(frozenset(ml))
            nonempty = [l for l in lists if l]
            if len(nonempty) > 1:
                multi_task_steps["steps_with_several_listing_tasks"] += 1
                if len(set(nonempty)) == len(nonempty) and not frozenset.intersection(*nonempty):
                    multi_task_steps["tasks_list_disjoint_notes"] += 1
                else:
                    multi_task_steps["tasks_share_notes"] += 1
            cur = frozenset().union(*lists) if lists else frozenset()
            if prev is not None and cur and prev:
                same_across_steps["overlap" if cur & prev else "disjoint"] += 1
            prev = cur
        for x in added:
            var_life["never_removed"] += 1

notes = [json.loads(l) for l in open(notes_p)]
how = collections.Counter()
flags_by_type = collections.defaultdict(collections.Counter)
for n in notes:
    t = n.get("template")
    info = tpl.get(t)
    ty = info["type"] if info else "unshipped"
    if not info:
        how[(ty, "template not shipped")] += 1
        continue
    for k, val in (n.get("flags") or {}).items():
        if val == "true":
            flags_by_type[ty][k] += 1
    if info["dnd"]:
        how[(ty, "do-not-display template")] += 1
    elif task_listed[t] and var_listed[t]:
        how[(ty, "task list and quest variable")] += 1
    elif task_listed[t]:
        how[(ty, "active task list")] += 1
    elif var_listed[t]:
        how[(ty, "quest map-note variable")] += 1
    elif info["cond"]:
        how[(ty, "static, condition-gated")] += 1
    else:
        how[(ty, "static")] += 1

tl_types = collections.Counter(tpl[m]["type"] if m in tpl else "unshipped" for m in task_listed)
vl_types = collections.Counter(tpl[m]["type"] if m in tpl else "unshipped" for m in var_listed)
cond_types = collections.Counter(i["type"] for i in tpl.values() if i["cond"])
cond_vals = collections.Counter(i["cond"] for i in tpl.values() if i["cond"])
dnd_types = collections.Counter(i["type"] for i in tpl.values() if i["dnd"])
dndm_types = collections.Counter(i["type"] for i in tpl.values() if i["dnd_mini"])
dnd_listed = sum(1 for m, i in tpl.items() if i["dnd"] and (task_listed[m] or var_listed[m]))

with open(f"{out}/mapnote_rule.txt", "w") as f:
    def p(*a):
        print(*a)
        print(*a, file=f)
    p("templates", len(tpl), "placed notes", len(notes))
    p("task-listed templates", len(task_listed), "by type", dict(tl_types))
    p("non-mpn task map-note entries (spawner/encounter notes)", dict(nonmpn))
    p("variable-listed templates", len(var_listed), "by type", dict(vl_types))
    p("map-note variable lifetime (remove step - add step, 5 = 5+)", dict(var_life), dict(var_where))
    p("multi-task steps", dict(multi_task_steps))
    p("consecutive steps' note sets", dict(same_across_steps))
    p("visibility condition by type", dict(cond_types))
    p("top conditions", cond_vals.most_common(12))
    p("do-not-display by type", dict(dnd_types), "of which quest-listed", dnd_listed)
    p("do-not-display-on-minimap by type", dict(dndm_types))
    p("placed notes: how shown")
    for k, v in sorted(how.items(), key=lambda kv: (str(kv[0][0]), -kv[1])):
        p("  ", k, v)
    p("placement flags (true) by type")
    for ty, c in sorted(flags_by_type.items(), key=lambda kv: str(kv[0])):
        p("  ", ty, dict(c))
