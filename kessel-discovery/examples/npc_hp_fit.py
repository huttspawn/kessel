"""NPC max HP from combat logs against cbtStandardHealingInfo: the multiplier bands per rank.

usage: python3 -I npc_hp_fit.py <fields.jsonl> <curves.jsonl> <log dir> [<log dir> ...]
fields.jsonl comes from the npc_hp_fields example, curves.jsonl from singleton_curves.
"""
import collections
import glob
import json
import os
import re
import statistics
import sys

ENTITY = re.compile(r"\[[^\]@\[|]*? ?\{(\d+)\}(?::\d+)?\|\([^)]*\)\|\(\d+/(\d+)\)")
TOUGHNESS, LEVEL_MIN, LEVEL_MAX = "618ec27c", "27bda14b", "27bda14c"
BRACKETS = ((11, 30), (31, 50), (51, 60), (61, 70), (71, 79), (80, 80))


def max_hp_by_name_id(log_dirs):
    """Name id (log id >> 32) to a count of each max HP seen. Logs are Windows-1252."""
    seen = collections.defaultdict(collections.Counter)
    for log_dir in log_dirs:
        for path in glob.glob(os.path.join(log_dir, "*.txt")):
            with open(path, "rb") as log:
                for line in log.read().decode("cp1252", "replace").splitlines():
                    for match in ENTITY.finditer(line):
                        seen[int(match.group(1)) >> 32][int(match.group(2))] += 1
    return seen


def standard_row(curves_path):
    """cbtStandardHealingInfo's level curve for toughness 2 (index 1), level 1 first."""
    with open(curves_path) as curves:
        for line in curves:
            curve = json.loads(line)
            if curve["fqn"] == "cbtStandardHealingInfo" and curve["path"] == "/5067c691/1":
                return curve["values"]
    raise SystemExit("cbtStandardHealingInfo /5067c691/1 not found")


def main():
    fields_path, curves_path, *log_dirs = sys.argv[1:]
    base = standard_row(curves_path)
    seen = max_hp_by_name_id(log_dirs)
    by_name = collections.defaultdict(list)
    with open(fields_path) as fields:
        for line in fields:
            npc = json.loads(line)
            by_name[npc["name"] >> 32].append(npc)
    # One NPC per name, one level, so the HP seen is that NPC's at that level.
    rows = collections.defaultdict(list)
    for name_id, counts in seen.items():
        candidates = by_name.get(name_id, [])
        if len(candidates) != 1:
            continue
        field = candidates[0]["f"]
        rank, level = field.get(TOUGHNESS), field.get(LEVEL_MIN)
        if not rank or level is None or level != field.get(LEVEL_MAX) or not 1 <= level <= 80:
            continue
        hp = counts.most_common(1)[0][0]
        if hp >= 20:
            rows[rank].append((level, hp / base[level - 1]))
    print(f"name ids in logs {len(seen)}, usable NPCs {sum(len(v) for v in rows.values())}")
    for rank in sorted(rows):
        for low, high in BRACKETS:
            ratios = [ratio for level, ratio in rows[rank] if low <= level <= high]
            if len(ratios) < 3:
                continue
            mode = collections.Counter(round(r, 3) for r in ratios).most_common(1)[0][0]
            near = sum(abs(r - mode) / mode < 0.08 for r in ratios)
            print(
                f"rank {rank} levels {low}-{high}: n={len(ratios)} mode={mode} "
                f"within 8%={near} ({100 * near // len(ratios)}%) "
                f"median={statistics.median(ratios):.3f}"
            )


if __name__ == "__main__":
    main()
