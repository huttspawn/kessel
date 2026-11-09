"""Turn the relations_fields sweep into docs/ontology/relations-fields.jsonl.

  python3 -I kessel-discovery/examples/relations_fields.py <raw.json> docs/fields/registry.jsonl \
      docs/ontology/relations-fields.jsonl <stats.json>

One line per (source class, field id, target class). Identity fields (a value naming the object
that holds it, in 90% or more of links) are left out and listed in the stats file instead.
"""
import json
import re
import sys
from collections import defaultdict

raw, registry_path, out_path, stats_path = sys.argv[1:5]
d = json.load(open(raw))
reg = {}
for line in open(registry_path):
    r = json.loads(line)
    reg[r["id"]] = r

# Hand-written verbs for relations whose field name alone does not say it. Keyed by
# (source class, field id, target class); everything else is phrased from the field name.
VERBS = json.load(open(__file__.replace(".py", "_verbs.json")))

cand = defaultdict(lambda: {"total": 0, "resolved": 0, "forms": defaultdict(int)})
for c in d["candidates"]:
    k = (c["source_class"], c["field_id"])
    cand[k]["total"] += c["total"]
    cand[k]["resolved"] += c["resolved"]
    cand[k]["forms"][c["form"]] += c["total"]


# Names proved by hash in client.gom, beside the registry (docs/fields/names.json).
NAMES = {}
for rec in json.load(open(registry_path.replace("registry.jsonl", "names.json")))["records"]:
    if rec.get("kind") == "field":
        NAMES[rec["id_hex"].lower()] = rec["name"]


def field_name(fid):
    r = reg.get(fid)
    if not r:
        return NAMES.get(fid), None, None
    name = NAMES.get(fid)
    if not name and r.get("name_hash"):
        name = r["meaning"]
    meaning = r.get("meaning")
    desc = None if meaning == name else meaning
    return name, desc, r.get("status")


def words(name):
    s = re.sub(r"^(cnv|qst|itm|abl|npc|plc|spn|enc|ach|cdx|hyd|tal|dis|schem|ipp|scs|stg|phs|mpn|epp)(?=[A-Z])", "", name)
    s = re.sub(r"([a-z0-9])([A-Z])", r"\1 \2", s).replace("_", " ").lower().strip()
    return s


def low(c):
    return c.split(":", 1)[1] if c.startswith("singleton:") else c


def verb(sc, fid, tc, name, desc):
    v = VERBS.get(f"{sc}|{fid}|{tc}") or VERBS.get(f"*|{fid}|{tc}") or VERBS.get(f"*|{fid}|*")
    if v:
        return v
    if name:
        return f"{low(sc)} names {low(tc)} as {words(name)}"
    if desc and not desc.startswith("reference to") and not desc.startswith("holds other"):
        short = desc.split(".")[0][:90]
        return f"{low(sc)} refers to {low(tc)} ({short})"
    return f"{low(sc)} refers to {low(tc)} (unnamed field)"


def form_label(forms):
    labels = {"guid": "GUID", "fqn": "FQN string", "fqn-member": "FQN string plus member suffix",
              "fqn-hash": "hash (event-code fold of the target FQN)", "class-ref": "class reference (GUID)"}
    return [labels[f] for f, _ in sorted(forms.items(), key=lambda x: -x[1])]


def position_label(pos):
    labels = {"field": "scalar", "list": "list element", "map-key": "map key", "map-value": "map value"}
    return [labels[p] for p, _ in sorted(pos.items(), key=lambda x: -x[1])]


rows, identity = [], []
for r in d["relations"]:
    sc, fid, tc = r["source_class"], r["field_id"], r["target_class"]
    if r["self_links"] >= 0.9 * r["links"]:
        identity.append(r)
        continue
    name, desc, rstatus = field_name(fid)
    c = cand[(sc, fid)]
    share = c["resolved"] / c["total"] if c["total"] else 0.0
    forms = r["forms"]
    main_form = max(forms, key=forms.get)
    # An exact 64-bit GUID or full-FQN match cannot be chance, so a field whose reference-shaped
    # values resolve 90%+ is proved whatever its count. A member suffix (qst.x.is_on_quest) is a
    # prefix match, so it is likely at best.
    # An icon or image field holds an asset name; where it equals an FQN the link is a naming
    # convention, not a reference the engine follows.
    asset = bool(name and re.search(r"Icon|Image|Background", name)) and main_form == "fqn"
    if share >= 0.9 and main_form != "fqn-member" and not asset:
        status = "proved"
    elif share >= 0.5 or main_form == "fqn-member" or (asset and share >= 0.9):
        status = "likely"
    else:
        status = "unsettled"
    paths = r["paths"]
    nested = [p for p, _ in paths if p]
    rows.append({
        "source_class": sc,
        "field_id": fid,
        "field_name": name,
        "field_meaning": desc,
        "registry_status": rstatus,
        "target_class": tc,
        "target_loaded": True,
        "meaning": verb(sc, fid, tc, name, desc) + (" (asset named like the target)" if asset else ""),
        "meaning_by": "hand" if any(k in VERBS for k in (f"{sc}|{fid}|{tc}", f"*|{fid}|{tc}", f"*|{fid}|*")) else "field name",
        "value_form": form_label(forms),
        "position": position_label(r["positions"]),
        "nested_path": paths[0][0] if paths and paths[0][0] else None,
        "nested_paths": r["path_count"],
        "per_source": "one" if r["max_per_source"] == 1 else "many",
        "per_source_max": r["max_per_source"],
        "per_source_mean": round(r["links"] / r["sources"], 2),
        "sources_per_target_max": r["max_sources_per_target"],
        "sources_per_target_mean": round(r["pairs"] / r["targets"], 2),
        "links": r["links"],
        "sources": r["sources"],
        "targets": r["targets"],
        "field_candidates": c["total"],
        "field_resolved": c["resolved"],
        "resolve_share": round(share, 4),
        "status": status,
        "sample": {"source": r["sample"][0], "target": r["sample"][1]},
    })

# A GOM GUID is the fold of its FQN, so an unresolved GUID that equals the fold of a string seen
# elsewhere in the data names something the extract does not load (wpn.*, med.*, a script event).
# Those get rows of their own with target_loaded false; per-source counts are not kept for them.
named = defaultdict(lambda: {"by_name": 0, "prefixes": defaultdict(int), "samples": []})
for c in d["candidates"]:
    k = (c["source_class"], c["field_id"])
    named[k]["by_name"] += c.get("unresolved_named_by_string", 0)
    for p, n in c.get("name_prefixes", {}).items():
        named[k]["prefixes"][p] += n
    named[k]["samples"] += c.get("name_samples", [])
for (sc, fid), x in list(named.items()):
    c = cand[(sc, fid)]
    share = c["resolved"] / c["total"] if c["total"] else 0.0
    name, desc, rstatus = field_name(fid)
    for p, n in x["prefixes"].items():
        tc = "unloaded:" + ("name" if p == "(text)" else p)
        sample = next((v for v in x["samples"] if (v.split(".", 1)[0] if "." in v else "(text)") == p), None)
        rows.append({
            "source_class": sc, "field_id": fid, "field_name": name, "field_meaning": desc,
            "registry_status": rstatus, "target_class": tc, "target_loaded": False,
            "meaning": f"{low(sc)} names a {tc.split(':', 1)[1]} the extract does not load"
                       + (f" ({words(name)})" if name else ""),
            "meaning_by": "field name",
            "value_form": ["GUID (fold of a name seen as a string elsewhere; no loaded object)"],
            "position": None, "nested_path": None, "nested_paths": None,
            "per_source": None, "per_source_max": None, "per_source_mean": None,
            "sources_per_target_max": None, "sources_per_target_mean": None,
            "links": n, "sources": None, "targets": None,
            "field_candidates": c["total"], "field_resolved": c["resolved"],
            "resolve_share": round(share, 4), "status": "likely",
            "sample": {"source": None, "target": sample},
        })

rows.sort(key=lambda x: (x["source_class"], x["field_id"], -x["links"]))
with open(out_path, "w") as f:
    for row in rows:
        f.write(json.dumps(row, ensure_ascii=False) + "\n")

# Reference-shaped values that resolve to nothing, per (source class, field).
unresolved = []
for (sc, fid), c in cand.items():
    miss = c["total"] - c["resolved"]
    if miss == 0:
        continue
    samples, prefixes = [], defaultdict(int)
    for x in d["candidates"]:
        if x["source_class"] == sc and x["field_id"] == fid:
            samples += x["unresolved_samples"]
            for p, n in x["unresolved_prefixes"]:
                prefixes[p] += n
    name, desc, _ = field_name(fid)
    unresolved.append({"source_class": sc, "field_id": fid, "field_name": name, "field_meaning": desc,
                       "candidates": c["total"], "unresolved": miss,
                       "named_unloaded": named[(sc, fid)]["by_name"] if (sc, fid) in named else 0,
                       "share_unresolved": round(miss / c["total"], 4),
                       "forms": dict(c["forms"]), "prefixes": dict(prefixes), "samples": samples[:4]})
unresolved.sort(key=lambda x: -x["unresolved"])

stats = {
    "objects": d["objects"], "read": d["read"], "failed": d["failed"],
    "relations": len(rows), "links": sum(r["links"] for r in rows),
    "loaded_relations": sum(1 for r in rows if r["target_loaded"]),
    "loaded_links": sum(r["links"] for r in rows if r["target_loaded"]),
    "unloaded_relations": sum(1 for r in rows if not r["target_loaded"]),
    "unloaded_links": sum(r["links"] for r in rows if not r["target_loaded"]),
    "by_status": {s: sum(1 for r in rows if r["status"] == s) for s in ("proved", "likely", "unsettled")},
    "links_by_status": {s: sum(r["links"] for r in rows if r["status"] == s) for s in ("proved", "likely", "unsettled")},
    "class_pairs": len({(r["source_class"], r["target_class"]) for r in rows}),
    "source_fields": len({(r["source_class"], r["field_id"]) for r in rows}),
    "field_ids": len({r["field_id"] for r in rows}),
    "source_classes": len({r["source_class"] for r in rows}),
    "target_classes": len({r["target_class"] for r in rows}),
    "identity": [{"source_class": r["source_class"], "field_id": r["field_id"],
                  "field_name": field_name(r["field_id"])[0], "links": r["links"],
                  "forms": r["forms"]} for r in sorted(identity, key=lambda r: -r["links"])],
    "unresolved": unresolved,
}
json.dump(stats, open(stats_path, "w"), indent=1)
print(json.dumps({k: v for k, v in stats.items() if k not in ("identity", "unresolved")}))
