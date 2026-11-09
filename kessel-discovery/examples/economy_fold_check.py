"""Check whether names exist as GOM nodes: GUID = 48-bit fold of FNV-1a 64 of the uppercased FQN.
Prints the control (real fqns) and the hit rate for mail.*, pkg.vendor.* and trp.* names."""
import os
import json, sqlite3
S = os.environ["KESSEL_SCRATCH"]  # dir holding s368/, b407/, r371/, dec-effects/ and dec-economy/
D = S + "/dec-economy"
M = (1 << 64) - 1


def fnv(s):
    h = 0xCBF29CE484222325
    for b in s.upper().encode():
        h = ((h ^ b) * 0x100000001B3) & M
    return h


def fold(h):
    return 0xE000000000000000 | (h & 0x0000FFFFFFFF0000) | ((h ^ (h >> 48)) & 0xFFFF)


u = sqlite3.connect(f"file:{S}/s368/unfiltered.sqlite?mode=ro", uri=True)
rows = u.execute("select guid, fqn from objects where is_canonical=1").fetchall()
guids = {int(g, 16) for g, _ in rows}
ok = sum(1 for g, f in rows if fold(fnv(f)) == int(g, 16))
print("control: canonical objects whose guid = fold(fqn):", ok, "of", len(rows))
b = sqlite3.connect(f"file:{S}/b407/spice.sqlite?mode=ro", uri=True)
mails = {r[0] for r in b.execute("select distinct target_fqn from script_actions where action='Send Mail' and target_fqn is not null")}
print("mail names", len(mails), "existing as nodes", sum(1 for m in mails if fold(fnv(m)) in guids))
vp = json.load(open(D + "/vendor_packages.json"))
print("vendor pkgs", len(vp), "existing as nodes", sum(1 for m in vp if fold(fnv(m)) in guids))
lh = json.load(open(D + "/loot_hashes.json"))
names = [v["name"][1] for v in lh.values() if v["name"]]
print("trp names", len(names), "existing as nodes", sum(1 for m in names if fold(fnv(m)) in guids))
tp = {r[0] for r in u.execute("select fqn from objects where fqn like 'pkg.profession_trainer.%'")}
print("trainer pkgs (control)", len(tp), "existing", sum(1 for m in tp if fold(fnv(m)) in guids))
