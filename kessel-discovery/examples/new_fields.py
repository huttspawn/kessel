"""List the field ids a new build's sweep has that the registry lacks, and the ones it no longer uses.

usage: python3 -I new_fields.py <fields.jsonl from field_sweep> docs/fields/registry.jsonl <gom_record_diff.json> <names.tsv> > new.jsonl

names.tsv holds proposed names, one '<HASH8> <name>' per line (gom_name_guess output, kept after
the owner and shape checks). Each output line is the sweep's entry for a new id, plus whether the
id is a record the new client.gom adds (or one it already had), its record hash and kind, and the
proposed name whose FNV-1a 32 equals that hash. A final line with "dropped" lists registry ids the
sweep no longer sees.
"""
import json
import sys


def fnv32(s):
    h = 0x811C9DC5
    for b in s.upper().encode():
        h = ((h ^ b) * 0x01000193) & 0xFFFFFFFF
    return h


def main():
    sweep_path, registry_path, gomdiff_path, names_path = sys.argv[1:]
    sweep = {}
    for line in open(sweep_path):
        f = json.loads(line)
        sweep[f["id"]] = f
    registry = {}
    for line in open(registry_path):
        r = json.loads(line)
        registry[r["id"]] = r
    gd = json.load(open(gomdiff_path))
    added = {r["id"]: r for r in gd["added"]}
    names = {}
    for line in open(names_path):
        parts = line.split()
        if len(parts) == 2 and fnv32(parts[1]) == int(parts[0], 16):
            names[parts[0].upper()] = parts[1]
    for fid in sorted(set(sweep) - set(registry)):
        f = dict(sweep[fid])
        rec = added.get(fid)
        f["client_record"] = "added in this build" if rec else "not a new record"
        if rec:
            f["record_hash"] = rec["hash"]
            f["record_kind"] = rec["kind"]
            f["proposed_name"] = names.get(rec["hash"])
        print(json.dumps(f, sort_keys=True))
    dropped = sorted(set(registry) - set(sweep))
    print(json.dumps({"dropped": [{"id": d, "meaning": registry[d].get("meaning"), "uses": registry[d].get("uses")} for d in dropped]}))


if __name__ == "__main__":
    main()
