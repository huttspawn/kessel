"""Writes one CloudEvent body per line from spice. @@ID@@ is filled at load, so ids carry load time."""
import json, re, sqlite3, sys

db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
out = open(sys.argv[2], "w")
CLASS = {"Npc": "npc", "Item": "item", "Quest": "quest", "Ability": "ability"}
PLANET = re.compile(r"^npc\.location\.([a-z0-9_]+)\.")


def node(cls, key, props, build):
    return {"specversion": "1.0", "id": "@@ID@@", "source": "kessel", "type": f"rannoch.node.{cls}",
            "subject": key, "data": {"properties": props, "axis": {"build": build}}}


planets = set()
links = []
restated = []
q = "select stable_id, fqn, kind, json from objects where is_canonical=1 and kind in ('Npc','Item','Quest','Ability')"
for i, (sid, fqn, kind, body) in enumerate(db.execute(q)):
    props = {"fqn": fqn, "json": body}
    out.write(json.dumps(node(CLASS[kind], sid, props, 1)) + "\n")
    if kind == "Npc":
        if i % 10 == 0:
            restated.append((sid, {"fqn": fqn, "json": body + " "}))
        m = PLANET.match(fqn)
        if m:
            planets.add(m.group(1))
            links.append((sid, m.group(1)))
for p in sorted(planets):
    out.write(json.dumps(node("planet", p, {"fqn": p, "json": ""}, 1)) + "\n")
for sid, p in links:
    out.write(json.dumps({"specversion": "1.0", "id": "@@ID@@", "source": "kessel", "type": "rannoch.link.on",
                          "data": {"from": {"class": "npc", "key": sid}, "to": {"class": "planet", "key": p},
                                   "axis": {"build": 1}}}) + "\n")
for sid, props in restated:
    out.write(json.dumps(node("npc", sid, props, 2)) + "\n")
