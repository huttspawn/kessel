"""Name unresolved 64-bit field values by hashing every string the client holds.

usage: python3 -I registry_hash_match.py <values.jsonl> <corpus files...> > hits.jsonl

values.jsonl comes from the field_hash_values example. Each candidate string is tried as
FNV-1a 64 of its uppercased form (signed or unsigned, the opaque-i64 rule) and as a GOM GUID
(0xE000 << 48 | 48-bit fold of that hash, reflection 01a1c265). Variants: the string itself,
with its first FQN segment dropped, its last segment, 'trp.' + tail (loot packages), and the
first segment swapped onto prefixes that hold no loaded object. Corpus files are JSON lines
(every string value inside is a candidate) or plain text (one candidate per line, first
tab-separated column).
"""
import json
import sys

M = 0xFFFFFFFFFFFFFFFF
PREFIXES = ["pkg", "wpn", "cbt", "spec", "abl", "npc", "itm", "class", "apn", "apc", "npp", "med", "sce",
            "veh", "app", "ipp", "epp", "stat", "mod", "trp", "spn", "enc", "phs", "qst", "cnd", "hyd",
            "ach", "mpn", "plc", "dyn", "cdx", "nco", "mrp", "tal", "dis", "schem", "dec", "tax", "rdd"]


def fnv(s):
    h = 0xCBF29CE484222325
    for b in s.encode("utf-8", "replace"):
        h ^= b
        h = (h * 0x100000001B3) & M
    return h


def fold(h):
    return 0xE000 << 48 | ((h >> 48) ^ (h & 0xFFFFFFFFFFFF))


def strings(v, out):
    if isinstance(v, str):
        out.add(v.lstrip("@"))
    elif isinstance(v, dict):
        for k, x in v.items():
            strings(x, out)
    elif isinstance(v, list):
        for x in v:
            strings(x, out)


def variants(s):
    yield s
    if "." in s and " " not in s:
        head, tail = s.split(".", 1)
        yield tail
        yield s.rsplit(".", 1)[1]
        yield "trp." + tail
        for p in PREFIXES:
            if p != head:
                yield p + "." + tail
    yield "trp." + s


def main():
    targets = {}
    for line in open(sys.argv[1]):
        r = json.loads(line)
        targets.setdefault(int(r["value"], 16), []).append(r["field"])
    corpus = set()
    for path in sys.argv[2:]:
        with open(path, errors="replace") as f:
            for line in f:
                line = line.rstrip("\n")
                if line.startswith("{") or line.startswith("["):
                    try:
                        strings(json.loads(line), corpus)
                        continue
                    except ValueError:
                        pass
                corpus.add(line.split("\t", 1)[0])
    print(f"{len(corpus)} corpus strings, {len(targets)} targets", file=sys.stderr)
    seen = set()
    for s in corpus:
        if not s or len(s) > 200:
            continue
        for v in variants(s):
            u = v.upper()
            if u in seen:
                continue
            seen.add(u)
            h = fnv(u)
            for value, how in ((h, "fnv64"), (fold(h), "guid-fold")):
                if value in targets:
                    print(json.dumps({"value": "%016x" % value, "fields": targets[value], "name": v, "how": how}))
    print(f"{len(seen)} variants hashed", file=sys.stderr)


if __name__ == "__main__":
    main()
