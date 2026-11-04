"""Builds the field registry: every field id the data uses, joined to what the client says of it.

usage: python3 -I field_registry.py <fields.jsonl> docs/fields/names.json docs/fields/registry.jsonl docs/fields/findings-*.json
fields.jsonl comes from the field_sweep example. Names count only when their FNV-1a hash matches
client.gom. A findings file maps a field id to {"meaning", "evidence", "confidence"} settled from
the data, or to {"reason"} when the data cannot settle it. Run from the repo root, which holds
kessel/resources.
"""
import collections
import json
import sys


def low32(full):
    return full[-8:]


def load_dictionary():
    """The client's own types: property id's high 32 bits are a field id's low 32 bits."""
    with open("kessel/resources/gom_properties.json") as f:
        return {p["id"][:8].lower(): p for p in json.load(f)}


def settle(field, name, prop, finding):
    """How far the evidence settles this field's meaning, and the evidence named."""
    if finding and "meaning" in finding:
        return f"decoded-{finding['confidence']}", finding["meaning"]
    if finding:
        return "unsettled", None
    refs = (prop or {}).get("refs") or []
    enum = next((r["name"] for r in refs if r.get("kind") == "enum"), None)
    shapes = field["shapes"]
    if name:
        return "named", name
    if enum:
        return "typed", f"value of enum {enum}"
    if field["ref_kinds"]:
        target = max(field["ref_kinds"], key=field["ref_kinds"].get)
        return "points-at", f"reference to {target} objects"
    if shapes.get("str") and any(p != "(text)" for p in field["prefixes"]):
        target = max(field["prefixes"], key=field["prefixes"].get)
        return "points-at", f"name of a {target} object"
    if set(shapes) <= {"list", "map", "object", "null"}:
        return "container", "holds other fields; its contents are their own entries"
    return "open", None


def main():
    fields_path, names_path, out_path, *finding_paths = sys.argv[1:]
    findings = {}
    for path in finding_paths:
        with open(path) as f:
            findings.update(json.load(f))
    dictionary = load_dictionary()
    with open(names_path) as f:
        names = {r["id_hex"].lower(): r for r in json.load(f)["records"]}
    tally = collections.Counter()
    uses = collections.Counter()
    with open(fields_path) as fields, open(out_path, "w") as out:
        for line in fields:
            field = json.loads(line)
            record = names.get(field["id"])
            name = (record or {}).get("name")
            prop = dictionary.get(low32(field["id"]))
            finding = findings.get(field["id"])
            status, meaning = settle(field, name, prop, finding)
            tally[status] += 1
            uses[status] += field["uses"]
            entry = {
                "id": field["id"],
                "status": status,
                "meaning": meaning,
                "evidence": (finding or {}).get("evidence"),
                "reason": (finding or {}).get("reason"),
                "name_hash": (record or {}).get("name_hash"),
                "name_proposed_by": (record or {}).get("name_proposed_by"),
                "in_client": record is not None,
                "client_kind": (prop or {}).get("kind"),
                **{k: field[k] for k in ("uses", "kinds", "parents", "shapes", "enums", "ref_kinds", "prefixes", "samples", "range")},
            }
            out.write(json.dumps(entry, sort_keys=True) + "\n")
    total = sum(uses.values())
    for status, count in tally.most_common():
        print(f"{status:10} {count:5} fields, {100 * uses[status] / total:6.2f}% of uses")


if __name__ == "__main__":
    main()
