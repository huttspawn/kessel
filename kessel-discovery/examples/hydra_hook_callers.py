"""Join effAction_ExecuteHydraHook to the Hydra handlers it fires.

An effect that runs ExecuteHydraHook {EventName X, ObjectSelf n} fires
'On Custom:X' in a Hydra script. With ObjectSelf 1 (hydObjectSelfEffect:
the effect itself) the script is the effect's own, named by its
effInitializer_SetHydraScript ScriptName; with 2 / 3 it is the effect
target's script (or its effects'), which the effect does not name.

Inputs: effects.jsonl (effect_catalog.py), Hydra.jsonl (dump_kind_jsonl on
an unfiltered extract), hidden_earnables.tsv (hidden_earnables.py).

Usage:
  python3 -I hydra_hook_callers.py <effects.jsonl> <Hydra.jsonl> <hidden_earnables.tsv> <out.tsv>
Prints how many hooks land on a real handler, and how many of the
'On Custom' achievement raisers with no decoded caller an effect now calls.
"""
import collections
import json
import sys


def handlers_of(v, out):
    if isinstance(v, dict):
        for x in v.values():
            handlers_of(x, out)
    elif isinstance(v, list):
        for x in v:
            if isinstance(x, dict) and set(x) == {"k", "v"} and isinstance(x["k"], str) \
                    and x["k"].startswith("On "):
                out.add(x["k"].lower())
            handlers_of(x, out)


def main():
    eff_path, hyd_path, he_path, out_path = sys.argv[1:5]
    handlers = {}
    for line in open(hyd_path):
        o = json.loads(line)
        hs = set()
        handlers_of(o["v"], hs)
        handlers.setdefault(o["fqn"].lower(), set()).update(hs)
    hooks = []  # (ability fqn, effect guid, script or None, event, object_self)
    for line in open(eff_path):
        r = json.loads(line)
        scripts = [i["params"].get("ScriptName") for b in r["blocks"] for i in b["init"]
                   if i["op"] == "SetHydraScript"]
        script = scripts[0] if scripts else None
        for b in r["blocks"]:
            for a in b["act"]:
                if a["op"] == "ExecuteHydraHook":
                    hooks.append((r["fqn"], r["guid"], script, a["params"].get("EventName"),
                                  a["params"].get("ObjectSelf")))
    st = collections.Counter()
    by_handler = collections.defaultdict(list)  # "script:event" -> callers
    by_event = collections.defaultdict(list)    # event -> callers (target-script hooks)
    for fqn, guid, script, ev, obj in hooks:
        st[f"object_self_{obj}"] += 1
        if script:
            st["names_script"] += 1
            hs = handlers.get(script.lower())
            if hs is None:
                st["script_not_extracted"] += 1
            elif f"on custom:{(ev or '').lower()}" in hs:
                st["lands_on_handler"] += 1
            else:
                st["script_has_no_such_handler"] += 1
            by_handler[f"{script}:{ev}".lower()].append((fqn, guid, obj))
        by_event[(ev or "").lower()].append((fqn, guid, script, obj))
    print(dict(st))

    rows = []
    orphan = found_exact = found_name = 0
    for line in list(open(he_path))[1:]:
        c = line.rstrip("\n").split("\t")
        for root in c[5].split(" | "):
            if "(no caller found)" not in root:
                continue
            orphan += 1
            script, handler = root.replace(" (no caller found)", "").split(" ", 1)
            ev = handler.split(":", 1)[1] if ":" in handler else ""
            exact = by_handler.get(f"{script}:{ev}".lower(), [])
            name_only = [x for x in by_event.get(ev.lower(), []) if x[2] is None]
            if exact:
                found_exact += 1
            elif name_only:
                found_name += 1
            rows.append([c[0], c[2], script, handler,
                         "exact" if exact else ("event-name, target script" if name_only else "none"),
                         " ".join(sorted({f for f, _, _ in exact} or {f for f, _, _, _ in name_only}))[:2000],
                         c[6]])
    with open(out_path, "w") as f:
        f.write("raiser_script\tevent\troot_script\troot_handler\tcaller_match\tcaller_abilities\tachievements\n")
        for r in rows:
            f.write("\t".join(r) + "\n")
    print(f"orphan roots {orphan}; called by an effect naming that script {found_exact}; "
          f"only by event name from a target-script hook {found_name}; "
          f"still none {orphan - found_exact - found_name}")


main()
