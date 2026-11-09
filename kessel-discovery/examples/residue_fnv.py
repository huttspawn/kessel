M = 0xFFFFFFFFFFFFFFFF


def fnv(s):
    h = 0xCBF29CE484222325
    for b in s.encode():
        h ^= b
        h = (h * 0x100000001B3) & M
    return h


def signed(h):
    return h - 2**64 if h >= 2**63 else h


def folds(s):
    h = fnv(s.upper())
    return {
        "xor16": 0xE000 << 48 | ((h >> 48) ^ (h & 0xFFFFFFFFFFFF)),
        "mask": 0xE000 << 48 | (h & 0xFFFFFFFFFFFF),
        "hi48": 0xE000 << 48 | (h >> 16),
        "xorhi": 0xE000 << 48 | (((h >> 16) ^ (h & 0xFFFF)) & 0xFFFFFFFFFFFF),
    }


if __name__ == "__main__":
    import sqlite3
    import sys

    db = sqlite3.connect(sys.argv[1])
    for f, g in db.execute("select fqn,guid from objects where is_canonical=1 limit 6"):
        print(f, g, {k: "%016X" % v for k, v in folds(f).items()})
