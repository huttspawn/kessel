"""Which NPC objects show up in combat logs as a player's companion, against an NPC flag.

usage: python3 -I registry_log_companions.py <Npc.jsonl> <spice.sqlite> <flag low32> <log files...>

A log entity '@Player/Name {id}:instance' is the player's companion; id >> 32 is the NPC's
string id (reflection 01a17a6a). Each companion string id is joined to the canonical Npc objects
carrying it, and split by whether those NPCs carry the flag. Also counts, among flagged NPCs, how
many string ids ever appear as a companion versus as a plain NPC.
"""
import collections
import json
import re
import sqlite3
import sys

COMPANION = re.compile(r"\[@[^|\]/]+/([^{\]]+) \{(\d+)\}")
PLAIN = re.compile(r"\] \[([^@\]|=][^{\]|]*) \{(\d+)\}:\d+\|")


def main():
    flag = sys.argv[3]
    has = {}
    for line in open(sys.argv[1]):
        r = json.loads(line)
        has[r["fqn"]] = flag in r["v"]
    db = sqlite3.connect(sys.argv[2])
    by_sid = collections.defaultdict(list)
    for fqn, sid in db.execute("select fqn, string_id from objects where kind='Npc' and is_canonical=1"):
        if sid and fqn in has:
            by_sid[sid].append(fqn)
    comp = collections.Counter()
    plain = collections.Counter()
    names = {}
    for path in sys.argv[4:]:
        with open(path, errors="replace") as f:
            for line in f:
                for name, i in COMPANION.findall(line):
                    comp[int(i) >> 32] += 1
                    names[int(i) >> 32] = name
                for name, i in PLAIN.findall(line):
                    plain[int(i) >> 32] += 1
                    names.setdefault(int(i) >> 32, name)
    table = collections.Counter()
    for sid in set(comp) | set(plain):
        fqns = by_sid.get(sid)
        if not fqns:
            continue
        state = {has[f] for f in fqns}
        key = "flagged" if state == {True} else "unflagged" if state == {False} else "mixed"
        role = "companion" if comp[sid] else "npc"
        table[key, role] += 1
        if key != "unflagged":
            print(key, role, names[sid], comp[sid], plain[sid], [f for f in fqns if has[f]][:2])
    print(dict(table))


if __name__ == "__main__":
    main()
