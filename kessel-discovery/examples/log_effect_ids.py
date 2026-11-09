"""Collect every effect a SWTOR combat log applies and join it to the
effect catalog (effect_catalog.py output).

A log line reads [ts] [source] [target] [ability {id}] [ApplyEffect
{836045448945477}: <name> {<effect id>}] ... The effect id is the effect's
name string id (id2 << 32 | id1), which the catalog stores as log_id.
Ids 836045448945500 (Heal) and 836045448945501 (Damage) are event kinds,
not effects, and are reported separately.

Usage:
  python3 -I log_effect_ids.py <effects.jsonl> <out.json> <log dir> [log dir...]
Prints how many distinct logged effect ids the catalog resolves, and writes
per-id rows (logged name, count, catalog matches) to out.json.
"""
import collections
import json
import os
import re
import sys

LINE = re.compile(
    r"^\[(?P<ts>\d+:\d+:\d+\.\d+)\] \[(?P<source>[^|\]]*)[^\]]*\] \[(?P<target>[^|\]]*)[^\]]*\] "
    r"\[(?P<ability>[^\]]*)\] \[(?P<ev>ApplyEffect|RemoveEffect) \{\d+\}: (?P<name>.*?) \{(?P<id>\d+)\}")
ABIL = re.compile(r"\{(\d+)\}")
EVENT_KINDS = {836045448945500, 836045448945501}


def main():
    cat_path, out_path, dirs = sys.argv[1], sys.argv[2], sys.argv[3:]
    by_name_id = collections.defaultdict(list)
    for line in open(cat_path):
        r = json.loads(line)
        for lid in r.get("log_ids") or []:
            by_name_id[lid].append(r)
    seen = collections.Counter()
    names = {}
    abilities = collections.defaultdict(collections.Counter)
    lasted = collections.defaultdict(list)  # effect id -> apply..remove ms
    files = 0
    for d in dirs:
        for fn in sorted(os.listdir(d)):
            if not fn.endswith(".txt"):
                continue
            files += 1
            open_at = {}
            with open(os.path.join(d, fn), encoding="utf-8", errors="replace") as fh:
                for line in fh:
                    m = LINE.search(line)
                    if not m:
                        continue
                    eid = int(m["id"])
                    if eid in EVENT_KINDS:
                        continue
                    h, mi, sec = m["ts"].split(":")
                    t = (int(h) * 3600 + int(mi) * 60 + float(sec)) * 1000
                    tgt = m["source"] if m["target"] == "=" else m["target"]
                    key = (m["source"], tgt, eid)
                    if m["ev"] == "RemoveEffect":
                        if key in open_at:
                            lasted[eid].append((t - open_at.pop(key)) % 86400000)
                        continue
                    # a repeated apply (refresh, zone change) keeps the first
                    open_at.setdefault(key, t)
                    seen[eid] += 1
                    names[eid] = m["name"]
                    a = ABIL.search(m["ability"])
                    if a:
                        abilities[eid][m["ability"]] += 1
    rows = []
    resolved = 0
    for eid, n in seen.most_common():
        cat = by_name_id.get(eid, [])
        if cat:
            resolved += 1
        rows.append({
            "id": eid, "logged_name": names[eid], "count": n,
            "abilities": abilities[eid].most_common(3),
            "lasted_ms_median": sorted(lasted[eid])[len(lasted[eid]) // 2] if lasted[eid] else None,
            "lasted_ms_max": max(lasted[eid]) if lasted[eid] else None,
            "catalog": [{"fqn": c["fqn"], "guid": c["guid"], "name": c["log_name"],
                         "from": c["log_id_from"],
                         "duration_ms": c["duration_ms"], "hidden": c["hidden"],
                         "actions": sorted({a["op"] for b in c["blocks"] for a in b["act"]
                                            if isinstance(a.get("op"), str)})} for c in cat[:8]],
        })
    # Duration check: for ids whose visible (not effIsHidden) catalog
    # effects agree on one duration D,
    # an effect that runs out is removed D after it was applied.
    ids_checked = full = 0
    for r in rows:
        ds = {c["duration_ms"] for c in r["catalog"] if c["duration_ms"] and not c["hidden"]}
        w = lasted[r["id"]]
        if len(ds) != 1 or not w:
            continue
        dur = ds.pop()
        near = sum(1 for x in w if abs(x - dur) <= 600)
        r["windows"], r["windows_at_duration"] = len(w), near
        r["windows_longer"] = sum(1 for x in w if x > dur + 600)
        ids_checked += 1
        full += near > 0
    json.dump(rows, open(out_path, "w"), indent=1)
    print(f"duration check: {ids_checked} ids with one catalog duration and a closed window; "
          f"{full} have at least one window within 0.6 s of it")
    print(f"files {files}; distinct ApplyEffect ids {len(seen)}; resolved {resolved}; "
          f"lines {sum(seen.values())}; lines resolved {sum(seen[r['id']] for r in rows if r['catalog'])}")


main()
