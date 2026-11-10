"""Who references the companion legacy unlock abilities? Search decoded dumps for their guids and FQNs.

usage: python3 -I companion_unlock_refs.py <unfiltered spice.sqlite> <file or dir> ...

Collects every guid of abl.legacy.companion_unlock.* rows (canonical and variant), as unsigned and signed
decimal and as @fqn, then scans each JSON/JSONL file (dirs: every *.json / *.jsonl inside) line by line and
prints which files and FQN-bearing lines mention one, and which unlock ability is named.
"""
import glob
import json
import os
import re
import sqlite3
import sys

db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
needles = {}
for fqn, guid in db.execute("select fqn, guid from objects where fqn like 'abl.legacy.companion_unlock.%'"):
    u = int(guid, 16)
    for n in (str(u), str(u - 2**64), "@" + fqn, guid, guid.lower()):
        needles[n] = fqn
pat = re.compile("|".join(re.escape(n) for n in sorted(needles, key=len, reverse=True)))
paths = []
for a in sys.argv[2:]:
    paths += sorted(glob.glob(os.path.join(a, "*.json*"))) if os.path.isdir(a) else [a]
for p in paths:
    with open(p, "rb") as fh:
        for n, raw in enumerate(fh):
            line = raw.decode("utf-8", "replace")
            hits = {needles[m] for m in pat.findall(line)}
            if not hits:
                continue
            fq = re.search(r'"fqn":\s*"([^"]+)"', line)
            print(f"{os.path.basename(p)}:{n + 1} {fq.group(1) if fq else ''} -> {sorted(hits)[:6]}")
