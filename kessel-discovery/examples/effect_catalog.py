"""Effect catalog: every effect an ability defines, one JSON line per effect.

An ability's effects are its same-FQN variant objects (listed in the
canonical row's 1d6b4918; each variant points back with 1d6b491c). A
variant that is an effect carries its own name and description string ids
(7af8a259, 7af8a25a: the id a combat log prints for ApplyEffect and
RemoveEffect), its number inside the ability (2efdc39f effNumber, which
CallEffect's EffectNumber names) and its duration (2f5d7053, ms). Its
behaviour is the sub-effect list e251d1b2: per entry, initializers e251d1c6
(enum e251d1d0 = effInitializer), triggers e251d1c3 (enum e251d1ce =
effTrigger), target overrides e251d1c1 (e251d1cf), conditions f1040f75
(enum e251d1cd = effCondition, combined by the postfix list 8cbfea50) and
actions e251d1bb (enum e251d1cc = effAction). Parameters are maps keyed by
effParam: e251d1d3 bools, e251d1d4 strings, e251d1d5 ints, e251d1d6 floats,
e27e11d1 durations ms, e251d1d7 tag sets (FNV hashes), 183ef1e8 / 8c989d91
/ b4ed1090 / 65bd7ee0 / dc9f9a89 / f70dd988 further typed maps.

The object decoder follows FR-KESSEL-001 (stream start from the header).

Usage:
  python3 -I effect_catalog.py <spice.sqlite> <gom_enums.json> <out.jsonl>
Every Ability row (canonical and variant) is read; rows without blocks and
without a name id are skipped.
"""
import base64
import collections
import json
import sqlite3
import struct
import sys


class Fail(Exception):
    pass


class R:
    def __init__(self, b, p):
        self.b, self.p = b, p

    def u8(self):
        if self.p >= len(self.b):
            raise Fail("eof")
        v = self.b[self.p]
        self.p += 1
        return v

    def be(self, n):
        if self.p + n > len(self.b):
            raise Fail("eof")
        v = int.from_bytes(self.b[self.p:self.p + n], "big")
        self.p += n
        return v

    def num(self):
        b = self.u8()
        if b == 0xD2:
            b = self.u8()
        if b <= 0xBF:
            return b
        if 0xC0 <= b <= 0xC7:
            return self.be(b - 0xBF)
        if 0xC8 <= b <= 0xCF:
            return self.be(b - 0xC7)
        if b == 0xD0:
            return self.u8()
        raise Fail("number prefix")

    def snum(self):
        b = self.u8()
        if b == 0xD2:
            n = self.u8()
            s = self.b[self.p:self.p + n]
            self.p += n
            return int(s.decode("ascii"))
        if b <= 0xBF:
            return b
        if 0xC0 <= b <= 0xC7:
            return -self.be(b - 0xBF)
        if 0xC8 <= b <= 0xCF:
            return self.be(b - 0xC7)
        if b == 0xD0:
            return 0
        raise Fail("signed prefix")

    def value(self, tag, key=False):
        if tag == 0x00 and key:
            return self.snum()
        if tag in (0x01, 0x0F, 0x14, 0x15, 0x0E):
            return {"ref": self.num()}
        if tag == 0x05:
            return {"enum": self.num() - 1}
        if tag == 0x02:
            return self.snum()
        if tag == 0x03:
            return self.u8() != 0
        if tag == 0x04:
            v = struct.unpack_from("<f", self.b, self.p)[0]
            self.p += 4
            return round(v, 6)
        if tag == 0x12:
            v = struct.unpack_from("<3f", self.b, self.p)
            self.p += 12
            return [round(x, 4) for x in v]
        if tag == 0x11:
            return [self.num() for _ in range(12)]
        if tag == 0x06:
            n = self.num()
            s = self.b[self.p:self.p + n].decode("utf-8", "replace")
            self.p += n
            return s
        if tag == 0x07:
            et = self.u8()
            n, n2 = self.num(), self.num()
            if n != n2:
                raise Fail("list length")
            out = []
            for _ in range(n):
                self.num()
                out.append(self.value(et))
            return out
        if tag == 0x08:
            kt, vt = self.u8(), self.u8()
            n, n2 = self.num(), self.num()
            if n != n2:
                raise Fail("map length")
            return [{"k": self.value(kt, True), "v": self.value(vt)} for _ in range(n)]
        if tag in (0x09, 0x18):
            return self.obj()
        raise Fail("tag")

    def obj(self):
        self.num()
        out, fid = {}, 0
        for _ in range(self.num()):
            fid = (fid + self.num()) & 0xFFFFFFFFFFFFFFFF
            out[f"{fid & 0xFFFFFFFF:08x}"] = self.value(self.u8())
        return out


TYPED_MAPS = ("e251d1d3", "e251d1d4", "e251d1d5", "e251d1d6", "e27e11d1",
              "183ef1e8", "8c989d91", "b4ed1090", "65bd7ee0", "dc9f9a89", "f70dd988")


def fnv1a64(b):
    h = 0xCBF29CE484222325
    for x in b:
        h = ((h ^ x) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


SLOT_HASHES = [fnv1a64(f"NAME{n}".encode()) for n in range(1, 33)]


def clean_text(v):
    if isinstance(v, str) and "<text>" in v:
        return v.split("<text>", 1)[1].split("</text>", 1)[0]
    return v


def main():
    db, enums_path, out_path = sys.argv[1:4]
    e = {x["name"]: x["members"] for x in json.load(open(enums_path))}
    P, A, C, I = e["effParam"], e["effAction"], e["effCondition"], e["effInitializer"]
    T, TO = e["effTrigger"], e["effTargetOverride"]
    con = sqlite3.connect(db)
    guid_fqn = {}
    for g, f in con.execute("select guid, fqn from objects"):
        try:
            guid_fqn[int(g, 16)] = f
        except ValueError:
            pass
    canon_sid = {}
    for f, sid in con.execute(
            "select fqn, string_id from objects where kind='Ability' and is_canonical=1 and string_id is not null"):
        canon_sid[f] = sid
    strings = {}
    for id1, id2, text in con.execute(
            "select id1, id2, text from strings where locale='en-us' and fqn like 'str.abl.%'"):
        strings[(id2 << 32) | id1] = text

    def pv(v):
        if isinstance(v, dict) and "ref" in v:
            r = v["ref"]
            return "@" + guid_fqn[r] if r in guid_fqn else r
        if isinstance(v, dict) and "enum" in v:
            return v["enum"]
        return clean_text(v)

    def unref(v):
        return v["ref"] if isinstance(v, dict) and "ref" in v else v

    def params(node):
        out = {}
        for key in TYPED_MAPS:
            for kv in node.get(key) or []:
                k = kv.get("k")
                if isinstance(k, dict) and 0 <= k.get("enum", -1) < len(P):
                    out[P[k["enum"]][9:]] = pv(kv.get("v"))
        tags = [kv.get("k") for kv in node.get("e251d1d7") or []]
        if tags:
            out["_tags"] = tags
        return out

    def entries(block, list_key, enum_key, names, prefix):
        out = []
        for n in block.get(list_key) or []:
            fid = None
            if isinstance(n, dict) and set(n) == {"k", "v"}:
                # effConditions is a map: function id -> condition
                fid, n = unref(n["k"]), n["v"]
            if not isinstance(n, dict):
                continue
            en = n.get(enum_key)
            idx = en.get("enum") if isinstance(en, dict) else None
            name = names[idx][len(prefix):] if idx is not None and 0 <= idx < len(names) else idx
            row = {"op": name, "params": params(n)}
            if "e7478982" in n or fid is not None:  # effFunctionId: what the postfix list names
                row["fid"] = unref(n.get("e7478982", fid))
            if n.get("34607a88"):  # effConditionFailureStringId
                row["fail_id"] = unref(n["34607a88"])
            out.append(row)
        return out

    def block_of(b):
        # One effSubEffects entry: initializers, triggers (effTrigger),
        # target overrides, conditions (effCondition, combined by the
        # postfix list 8cbfea50 of (type e71e2f92, id e71f8f23)), actions.
        return {
            "init": entries(b, "e251d1c6", "e251d1d0", I, "effInitializer_"),
            "trig": entries(b, "e251d1c3", "e251d1ce", T, "effTrigger_"),
            "target": entries(b, "e251d1c1", "e251d1cf", TO, "effTargetOverride_"),
            "cond": entries(b, "f1040f75", "e251d1cd", C, "effCondition_"),
            "postfix": [[pv(x.get("e71e2f92")), unref(x.get("e71f8f23"))]
                        for x in b.get("8cbfea50") or [] if isinstance(x, dict)],
            "act": entries(b, "e251d1bb", "e251d1cc", A, "effAction_"),
        }

    def decode(js):
        d = json.loads(js)
        p, h = base64.b64decode(d["payload_b64"]), bytes.fromhex(d["header_hex"])
        st = 0
        ps, ng, so = (struct.unpack_from("<H", h, o)[0] for o in (10, 28, 36))
        if ng <= 64 and 0 <= so - ps < len(p):
            st = so - ps
        return R(p, st).obj()

    # The canonical row's locTextRetrieverMap (a787ee87) is keyed by
    # FNV-1a 64 of an upper-case slot name: "NAME" / "DESCRIPTION" for the
    # ability, "NAME<n>" / "DESCRIPTION<n>" for its effect number n.
    loc = {}
    for fqn, js in con.execute(
            "select fqn, json from objects where kind='Ability' and is_canonical=1"):
        try:
            f = decode(js)
        except (Fail, ValueError, UnicodeDecodeError, struct.error):
            continue
        m = {}
        for kv in f.get("a787ee87") or []:
            v = kv.get("v")
            if isinstance(v, dict) and "5ce87488" in v:
                m[kv.get("k")] = unref(v["5ce87488"])
        loc[fqn] = m

    out = open(out_path, "w")
    counts = collections.Counter()
    for game_id, guid, fqn, canon, js in con.execute(
            "select game_id, guid, fqn, is_canonical, json from objects where kind='Ability'"):
        try:
            f = decode(js)
        except (Fail, ValueError, UnicodeDecodeError, struct.error):
            counts["fail"] += 1
            continue
        blocks = f.get("e251d1b2") or []
        name_id = unref(f.get("7af8a259"))
        if not blocks and name_id is None:
            counts["no_blocks"] += 1
            continue
        counts["effects"] += 1
        parent = f.get("1d6b491c")
        built = [block_of(b) for b in blocks if isinstance(b, dict)]
        set_name = next((i["params"].get("StringId") for b in built for i in b["init"]
                         if i["op"] == "SetName"), None)
        # The id a combat log prints: the effect's own name id (7af8a259,
        # always equal to its SetName StringId), else the owning ability's
        # current name. That is NAME (string key 0), or one of the alternate
        # tooltip slots NAME1..NAMEn the ability carries for disciplines and
        # talents (Overcharge Saber: NAME3 = 320 under Lightning); the log
        # prints whichever slot is active, so all of them are kept.
        num = f.get("2efdc39f")
        lm = loc.get(fqn, {})
        slots = [lm[h] for h in SLOT_HASHES if h in lm]
        if name_id and set_name and name_id != set_name:
            counts["name_id_vs_setname_conflict"] += 1
        if name_id:
            log_id, how, log_ids = name_id, "7af8a259", [name_id]
        elif set_name:
            log_id, how, log_ids = set_name, "SetName", [set_name]
        elif canon_sid.get(fqn):
            log_id, how = canon_sid[fqn] << 32, "ability"
            log_ids = list(dict.fromkeys([log_id] + slots))
        else:
            log_id, how, log_ids = None, None, []
        counts[f"log_id_from_{how}"] += 1
        if how == "ability" and len(log_ids) > 1:
            counts["ability_named_with_slots"] += 1
        rec = {
            "fqn": fqn, "guid": guid, "game_id": game_id, "canonical": canon,
            "parent": pv(parent) if parent is not None else None,
            "name_id": name_id, "name": strings.get(name_id) if name_id else None,
            "desc_id": unref(f.get("7af8a25a")),
            "desc": strings.get(unref(f.get("7af8a25a"))),
            "log_id": log_id, "log_id_from": how, "log_ids": log_ids,
            "log_name": strings.get(log_id) if log_id else None,
            "eff_number": f.get("2efdc39f"),
            "duration_ms": unref(f.get("2f5d7053")),
            "hidden": bool(f.get("2f5d7051")),
            "slot_type": pv(f.get("395b948e")) if f.get("395b948e") is not None else None,
            "icon": f.get("f293eb69"),
            "blocks": built,
        }
        out.write(json.dumps(rec) + "\n")
    print(dict(counts))


main()
