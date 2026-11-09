"""Try to name the two default spawn/encounter timer hashes (B6)."""
import json
import re
import sys

sys.path.insert(0, ".")
from residue_fnv import fnv, folds  # noqa: E402

T = {0xE23DDC377CB394AB: "spawn 15ac2498 default", 0x270299ED9A3CEA3E: "encounter 15ac249b default"}
words = set()
names = json.load(open(sys.argv[1]))
for v in (names.values() if isinstance(names, dict) else names):
    for t in re.findall(r"[A-Za-z]+", json.dumps(v)):
        words.add(t.upper())
for t in "SPN ENC CRF CRE PLC DEFAULT AGGRO AGG RESPAWN SURVIVE SURVIVAL REFILL DISPENSE TIMER DELAY NEVER NONE NULL INFINITE INSTANT SECONDS SECOND MINUTES MINUTE HOURS HOUR MS SEC MIN NO RESET DESPAWN DEATH WAIT".split():
    words.add(t)
pre = ["", "SPN_", "ENC_", "CRF_", "CRE_", "PLC_", "TMR_", "TIMER_"]
suf = ["", "_DEFAULT", "_TIMER", "_DELAY"]
cands = set()
for w in words:
    for p in pre:
        for s in suf:
            cands.add(p + w + s)
for n in range(0, 100001):
    s = str(n)
    cands.update({s, s + " SECONDS", s + " SECOND", s + " MINUTES", s + " MINUTE", s + "S", s + "M", s + " MS",
                  "SPN_" + s, "ENC_" + s, "CRF_" + s, s + ".0", "-" + s})
for w in ["SPN_AGGRO_DEFAULT", "SPN_SURVIVE_DEFAULT", "ENC_SURVIVE_DEFAULT", "AGGRO_RESPAWN_DEFAULT", "DEFAULT_TIMER"]:
    cands.add(w)
cands.add("")
print("candidates", len(cands))
for c in cands:
    h = fnv(c)
    if h in T:
        print("HIT", T[h], repr(c))
    if folds(c)["xor16"] in T:
        print("FOLD HIT", repr(c))
print("done")
