"""Diff the type records of two client.gom files: the ids one build adds or drops.

usage: python3 -I gom_record_diff.py <old client.gom> <new client.gom> <names.json> > diff.json

Walks every DBLB v2 record (u32 length, u32 name hash, u64 id, u16 flags, u16 name offset at 0x14),
the same walk as recover_gom_names.rs. A record's kind is (flags >> 3) & 0xF. Type records keep
FNV-1a 32 of their uppercased name and a blank name; instance records keep a plain name and hash 0.
Prints, for each id only one side has, its kind, hash and plain name, and for ids on both sides
whose hash changed (a renamed field), both hashes. names.json is docs/fields/names.json, used to
label known ids.
"""
import json
import struct
import sys


def walk(path):
    gom = open(path, "rb").read()
    off = gom.index(b"DBLB") + 8
    out = {}
    while off + 24 <= len(gom):
        (length,) = struct.unpack_from("<I", gom, off)
        if length == 0:
            break
        rec = gom[off : off + length]
        name_hash, lo, hi, flags = struct.unpack_from("<IIIH", rec, 4)
        (name_off,) = struct.unpack_from("<H", rec, 0x14)
        name = None
        if 0 < name_off < length:
            end = rec.find(b"\0", name_off)
            name = rec[name_off : end if end >= 0 else length].decode("latin-1") or None
        rid = lo | (hi << 32)
        out[rid] = {"hash": f"{name_hash:08X}", "kind": (flags >> 3) & 0xF, "plain": name}
        off += length + ((8 - (length & 7)) & 7)
    return out


def main():
    old, new, names_path = sys.argv[1:]
    a, b = walk(old), walk(new)
    names = {int(r["id_hex"], 16): r["name"] for r in json.load(open(names_path))["records"]}

    def row(rid, r):
        return {"id": f"{rid:016x}", "low32": f"{rid & 0xFFFFFFFF:08x}", **r, "known_name": names.get(rid)}

    result = {
        "old_records": len(a),
        "new_records": len(b),
        "added": [row(i, b[i]) for i in sorted(set(b) - set(a))],
        "dropped": [row(i, a[i]) for i in sorted(set(a) - set(b))],
        "rehashed": [
            {"id": f"{i:016x}", "old": a[i]["hash"], "new": b[i]["hash"], "known_name": names.get(i)}
            for i in sorted(set(a) & set(b))
            if a[i]["hash"] != b[i]["hash"]
        ],
    }
    json.dump(result, sys.stdout, indent=1)


if __name__ == "__main__":
    main()
