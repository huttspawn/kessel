"""Test ability flag 4000000197d6d791 (breaks stealth) against combat logs.

usage: python3 -I registry_stealth_break.py <Ability.jsonl> <spice.sqlite> <log files...>

A stealth interval runs from ApplyEffect Stealth on the player to its end (RemoveEffect Stealth on
the player or their companion, the Stealth ability deactivating, death or an area change); open
intervals are dropped. In a closed interval the last activation broke stealth when the end came
within 0.6 s of it; every other activation kept stealth. Log ability ids join to abilities by id >> 32 = objects.string_id (reflection
01a17a6a). Prints, per ability name, the flag values its canonical abilities carry and the
break / keep counts, then the agreement totals.
"""
import collections
import json
import re
import sqlite3
import sys

LINE = re.compile(r"^\[(\d+):(\d+):(\d+)\.(\d+)\] \[([^\]]*)\] \[([^\]]*)\] \[([^\]]*)\] \[([^\]]*)\]")
ID = re.compile(r"\{(\d+)\}")


def seconds(m):
    return int(m[1]) * 3600 + int(m[2]) * 60 + int(m[3]) + int(m[4]) / 1000


def main():
    flagged = {}
    for line in open(sys.argv[1]):
        r = json.loads(line)
        flagged[r["fqn"]] = bool(r["v"].get("97d6d791"))
    db = sqlite3.connect(sys.argv[2])
    flags_by_sid = collections.defaultdict(set)
    for fqn, sid in db.execute("select fqn, string_id from objects where kind='Ability' and is_canonical=1"):
        if fqn in flagged and sid:
            flags_by_sid[sid].add(flagged[fqn])
    breaks = collections.Counter()
    keeps = collections.Counter()
    names = {}
    gaps = collections.defaultdict(list)
    for path in sys.argv[3:]:
        stealthed = {}
        with open(path, errors="replace") as f:
            for raw in f:
                m = LINE.match(raw)
                if not m:
                    continue
                t = seconds(m)
                src, tgt, abl, eff = m[5], m[6], m[7], m[8]
                if not src.startswith("@") or "/" in src.split("|")[0]:
                    continue
                who = src.split("|")[0]
                tname = tgt.split("|")[0]
                own = tgt == "=" or tname == who or tname.startswith(who + "/")
                ends = (
                    ("RemoveEffect" in eff and "Stealth {" in eff and own)
                    or (abl.startswith("Stealth {") and "AbilityDeactivate" in eff)
                    or "AreaEntered" in eff
                    or ("Death {" in eff and tgt == "=")
                )
                if ends:
                    # Close the interval: the last activation broke stealth if the end came
                    # within 0.6 s; every earlier activation in the interval kept it.
                    acts = stealthed.pop(who, None)
                    if acts:
                        for aid, at in acts:
                            gaps[aid].append(round(t - at, 1))
                        for aid, at in acts[:-1]:
                            keeps[aid] += 1
                        aid, at = acts[-1]
                        if t - at <= 0.6 and "AreaEntered" not in eff and "Death {" not in eff:
                            breaks[aid] += 1
                        else:
                            keeps[aid] += 1
                    continue
                if "ApplyEffect" in eff and "Stealth {" in eff and own:
                    stealthed.setdefault(who, [])
                    continue
                if "AbilityActivate" in eff and who in stealthed:
                    ids = ID.findall(abl)
                    if ids and not abl.startswith("Stealth "):
                        aid = int(ids[0])
                        names[aid] = abl.split(" {")[0]
                        stealthed[who].append((aid, t))
    agree = disagree = unknown = 0
    for aid in sorted(set(breaks) | set(keeps), key=lambda a: names[a]):
        flags = flags_by_sid.get(aid >> 32, set())
        b, k = breaks[aid], keeps[aid]
        verdict = "break" if b > k else "keep"
        if len(flags) == 1:
            ok = (True in flags) == (verdict == "break")
            agree += ok
            disagree += not ok
        else:
            unknown += 1
        g = sorted(gaps[aid])
        print(f"{names[aid]:40} flag={sorted(flags)} break={b} keep={k} seconds-to-end min/median={g[0]}/{g[len(g) // 2]}")
    print(f"agree {agree}, disagree {disagree}, unjoined or mixed {unknown}")
    # Activations whose interval ended within 30 s, so stealth was surely on when they fired.
    live = collections.Counter()
    for aid, g in gaps.items():
        flags = flags_by_sid.get(aid >> 32, set())
        key = "flagged" if flags == {True} else "unflagged" if flags == {False} else "mixed"
        live[key, "live"] += sum(1 for x in g if 0 <= x <= 30)
        live[key, "broke"] += breaks[aid]
    print("activations inside a stealth interval that ended within 30 s:", dict(live))


if __name__ == "__main__":
    main()
