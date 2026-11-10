#!/usr/bin/env python3
"""Decode push 3, world: every place a Hydra script names a quest, with the action around it.

For each decoded Hydra object (dump_kind_jsonl Hydra output), walks the handler map
(hydScriptMap e389e2e3) down to each action dict (hydAction e5346fa0) and reports every
action whose values name one of the given quests, with handler, action name, the field
the quest sits in and the action's other fields. With --all, tallies, over every script,
which (action, field) pairs carry a qst reference, so a form script_actions misses shows
against the forms it reads.

Usage: world3_hydra_quest_refs.py <Hydra.jsonl> [--all] [quest fqn ...]
"""
import collections
import json
import sys

path = sys.argv[1]
tally = "--all" in sys.argv
want = {a for a in sys.argv[2:] if not a.startswith("--")}

ACTION = "e5346fa0"
forms = collections.Counter()
examples = {}


def refs(v, field, out):
    if isinstance(v, dict):
        for k, x in v.items():
            if isinstance(k, str) and k.startswith("@qst."):
                out.append((field + "{key}", k[1:]))
            refs(x, k if field == "" else field, out)
    elif isinstance(v, list):
        for x in v:
            refs(x, field, out)
    elif isinstance(v, str) and v.startswith("@qst."):
        out.append((field, v[1:]))
    elif isinstance(v, str) and v.startswith("qst."):
        out.append((field + "(text)", v))


def actions(v, handler, sink):
    if isinstance(v, dict):
        if ACTION in v:
            sink.append((handler, v))
        for x in v.values():
            actions(x, handler, sink)
    elif isinstance(v, list):
        for x in v:
            actions(x, handler, sink)


for line in open(path):
    o = json.loads(line)
    fqn = o.get("fqn", "")
    v = o.get("v", o)
    for h in v.get("e389e2e3", []) or []:
        if not isinstance(h, dict):
            continue
        sink = []
        actions(h.get("v"), h.get("k"), sink)
        for handler, a in sink:
            out = []
            for k, x in a.items():
                if k != ACTION:
                    refs(x, k, out)
            for field, q in out:
                forms[(a.get(ACTION), field)] += 1
                examples.setdefault((a.get(ACTION), field), (fqn, handler, q))
                if q in want:
                    print(json.dumps({"script": fqn, "handler": handler, "quest": q,
                                      "field": field, "action": a}, sort_keys=True)[:3000])

if tally:
    for (act, field), n in forms.most_common():
        print(n, act, field, examples[(act, field)])
