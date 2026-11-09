"""B9 third pass: test readings. 0x100 text, 0x400 voiced, 0x1000 comm filter."""
import collections
import os
import re
import struct
import sys

root = sys.argv[1]
c = collections.defaultdict(collections.Counter)
for dp, _, fs in os.walk(root):
    for f in fs:
        if not f.endswith(".stb"):
            continue
        p = os.path.join(dp, f)
        d = open(p, "rb").read()
        if len(d) < 7 or d[0] != 1:
            continue
        n = struct.unpack_from("<I", d, 3)[0]
        rel = p[len(root):]
        cnv = "/str/cnv/" in rel
        holo = bool(re.search(r"holo|transmission|comm|radio|broadcast|announc|intercom|_tx", rel))
        alien = "/alien_vo/" in rel
        for i in range(n):
            o = 7 + 26 * i
            id1, id2, lo, hi, ver, ln, off, ln2 = struct.unpack_from("<IIBBIIII", d, o)
            txt = d[off:off + ln].decode("utf-8", "replace") if ln else ""
            k = "cnv" if cnv else "other"
            for b, name in ((1, "0x100"), (4, "0x400"), (8, "0x800"), (0x10, "0x1000")):
                s = bool(hi & b)
                if lo == 0x41:
                    c[(k, name, s)]["text" if txt else "empty"] += 1
                    if cnv and txt:
                        c[(name, s)]["bracket" if txt.lstrip().startswith("[") else "speech"] += 1
                        c[(name, s, "holo")]["holo_file" if holo else "plain_file"] += 1
                    if alien:
                        c[(name, s, "alien")]["alien"] += 1
for k in sorted(c, key=str):
    print(k, dict(c[k]))
