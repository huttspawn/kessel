"""Push-3 fields pass (#429): settle every decoded-likely and unsettled field in docs/fields/findings-*.json.

usage: python3 -I kessel-discovery/examples/registry_settle.py kessel-discovery/examples/registry_settle_decisions.json

Run from the repo root. Reads the registry for the field list; writes findings files in place.
Prints counts by outcome and the skipped fields.
"""
import collections
import glob
import json
import re
import sys

REG = "docs/fields/registry.jsonl"

# Fields owned by other push-3 groups (itemization, map notes).
SKIP = {
    # itemization (criterion 3: shell stats, modifier packages, cbtStandardRatingInfo, relic curve)
    "40000044dcbfd721": "itemization", "40000044dcbfd722": "itemization", "40000044dcbfd723": "itemization",
    "4000002f55809161": "itemization", "400001057c8da5f0": "itemization", "400000445dc59599": "itemization",
    "40000004d73288a8": "itemization", "400003d2a9eaf0f2": "itemization", "40000040eded80bd": "itemization", "4000000a384b793c": "itemization (lives in findings-items.json)",
    # map notes (mpn templates and quest map-note variables)
    "4000000de986726f": "map notes", "40000007d852e845": "map notes", "40000034e7e45c30": "map notes",
    "4000004267d92710": "map notes", "40000034c8b89fb2": "map notes", "40000034c8b89fb3": "map notes",
}

LOGS = "Sean's 264 combat logs"
WHY = {
    "spawn": ("Not determinable from the client: spawners and encounters run on the server. The client ships no "
              "spawner runtime or spawn definitions (.spn_c, .spn_crf and .spn_lst are not in the archives, RESEARCH "
              "01a1c274-edd7 and 01a1c0be-9414), and combat logs record no spawn, despawn, respawn, timer or slot "
              "events, so the files hold this value and nothing that consumes or reports it.",
              "Timing or watching a spawner that carries the value beside one that does not, in game, or the "
              "server's spawner data, would determine it."),
    "npc": ("Not determinable from the client: NPC rules are applied by the server. Combat logs give an NPC's name, "
            "id, position and health but no level, XP, loot, aggression, faction or companion state, and nothing "
            "else in the client data refers to this value.",
            "An in-game comparison of an NPC that carries the value with one that does not, or server data, "
            "would determine it."),
    "ai": ("Not determinable from the client: package ability entries steer server-side NPC and companion AI and "
           "ability grants. Combat logs show which ability an NPC or companion fires and its health, not the "
           "priority, wave, gate or grant rule behind it; " + LOGS + " hold 35 activations of abilities with a "
           "single health gate (kessel-discovery/examples/registry_ai_health_gate.py: 24 at or below the gate, 11 "
           "above, all 11 on one shared-name ability), too few to measure any of these rules.",
           "Logging enough fights against NPCs that use these packages to test each value, or server AI data, "
           "would determine it."),
    "quest": ("Not determinable from the client: quest and conversation flow runs on the server. The client holds "
              "the quest data but not the runtime that reads this value, and combat logs print no quest, task, "
              "conversation, variable or mission-log events.",
              "Playing a quest step that carries the value against one that does not, or server quest data, would "
              "determine it."),
    "hydra": ("Not determinable from the client: Hydra scripts and their triggers run on the server. The client "
              "holds the script data but no trigger runtime, and combat logs print no trigger, volume or script "
              "events.",
              "Crossing a scripted volume that carries the value in game, beside one that does not, or server "
              "script logs, would determine it."),
    "object": ("Not determinable from the client: object use, collision, loot and morph rules are run by the server "
               "or by client code in the executable, which kessel does not reverse. Nothing else in the client data "
               "refers to this value, and combat logs print no use, loot, collision, mount or morph events.",
               "Using an object that carries the value beside one that does not, in game, or the executable's "
               "handler for it, would determine it."),
    "ability": ("Not determinable from the client: the ability system applies this value in server or executable "
                "code. Combat logs show activations, effects and stealth, not mount state, quickbar placement, "
                "cooldown sharing, targeting or bolster rules, and nothing else in the client data refers to it.",
                "Using an ability that carries the value beside one that does not, in game, would determine it."),
    "path": ("Not determinable from the client: patrol movement runs on the server, and combat logs give "
             "positions only on combat lines, so a path's traversal and waits never show.",
             "Watching a patrol on a path that carries the value, in game, would determine it."),
    "phase": ("Not determinable from the client: instance phases, difficulty modes, lockouts and operation states "
              "are server-run. Combat logs name the area and difficulty on AreaEntered but print no phase state, "
              "lockout, bolster or level-sync line.",
              "Entering the instance in each mode and reading the result in game, or server instance data, would "
              "determine it."),
    "gsf": ("Not determinable from the client data: GSF flight, camera, weapon, visual and reward tuning is read by "
            "the client's flight code in the executable, which kessel does not reverse, or by server match rules. "
            "The data files hold the number only, and combat logs print no camera, flight-model, requisition or "
            "ship-tuning lines.",
            "Flying GSF and measuring the effect, or reversing the executable's flight code, would determine it."),
    "pvp": ("Not determinable from the client: warzone, arena and matchmaking rules are server-run. Combat logs show "
            "no warzone score, round, respawn-wave or queue lines (RESEARCH 01a1c276-cbce), and nothing else in the "
            "client data refers to this value.",
            "One recorded match or queue with a stopwatch, comparing the rule this value is suspected of, would "
            "determine it."),
    "combat": ("Not determinable from the client: this is a combat-system constant applied by server code. Combat "
               "logs carry no levels, defence breakdowns, leash, pursuit, pushback or targeting decisions, and "
               "nothing else in the client data refers to it.",
               "A controlled in-game test of the rule this value is suspected of, or server code, would determine "
               "it."),
    "xp": ("Not determinable from the client: XP is granted by the server and combat logs print no XP lines "
           "(checked across " + LOGS + "); nothing else in the client data refers to these columns.",
           "Reading the XP gained from each source at a few levels in game would determine it."),
    "progression": ("Not determinable from the client data: the account, legacy, progression, store and UI rules "
                    "that read this value are server code or executable code. Nothing else in the client data "
                    "refers to it, no STB the client ships labels it, and combat logs print no XP, rank, currency, "
                    "store, title or UI events.",
                    "An in-game check of the feature this value belongs to, or server data, would determine it."),
    "area": ("Not determinable from the client: area rules (load and awareness ranges, sync, dueling, PvP class, "
             "chat zones) are applied by server code and by the executable. Combat logs print only AreaEntered "
             "lines for an area, and nothing else in the client data refers to this value.",
             "Comparing two areas that differ in the value, in game, would determine it."),
    "stub": ("Not determinable from the client data: these are engine prototype stub singletons with one row each, "
             "and no object instantiates them, so nothing in the data ever uses the value.",
             "Only the engine code in the executable could say what the default controls."),
}


def category(r):
    kinds = list(r["kinds"])
    k = kinds[0]
    s = k.replace("singleton:", "")
    if s.startswith("Proto") and s.endswith("Instance"):
        return "stub"
    if k in ("Spawn", "Encounter"):
        return "spawn"
    if s.startswith(("sysAreaInfo", "mapAreasData")):
        return "area"
    if k in ("Npc", "class", "NpcPackage") or s.startswith(("chrFactionPackages", "chrCompanionInfo")):
        return "npc"
    if k in ("AnimationPackage", "apc"):
        return "ai"
    if k in ("Quest", "Conversation", "Condition") or s.startswith(("qstRewards", "cnvReaction", "cnvHolo",
                                                                     "itmGift", "chrCompanionAffection")):
        return "quest"
    if k == "Hydra":
        return "hydra"
    if k in ("Placeable", "Dynamic", "dec", "MountPackage", "Item", "schem", "tax"):
        return "object"
    if k in ("Ability", "Talent"):
        return "ability"
    if k == "pth":
        return "path"
    if k in ("phs", "rdd", "Phase"):
        return "phase"
    if k in ("chase", "freelook", "spvp", "tl") or s.startswith(("sc", "BlasterVisual", "veh", "spnVehicle")) \
            or "eppdynamicdata" in s:
        return "gsf"
    if s.startswith(("dcvPVP", "dcvKingOfTheHill", "arb", "mms", "pvp", "wz")):
        return "pvp"
    if s.startswith(("dcvCombat", "ablPushback", "animLoco")):
        return "combat"
    if s.startswith(("lgcLevelAndXp", "qstExperience")):
        return "xp"
    return "progression"


WHAT_RE = re.compile(r"would (settle|show|help|be needed|test|tell|separate|name|confirm|prove|determine)|"
                     r"settle it|in game|in-game|server (data|code|source|log|table)", re.I)


CONST_RE = re.compile(r"constant|on all [0-9,]+|always true|only ever|^single|one use|engine stub", re.I)


def nd_clause(r, cat, have_what, text=""):
    why, what = WHY[cat]
    if have_what:
        return why
    if CONST_RE.search(text) and cat != "stub":
        what = ("The value never varies in this build, so only server or executable code, or a build or object "
                "where it differs, would determine it.")
    return why + " " + what


def main():
    decisions = json.load(open(sys.argv[1]))  # field id -> {"proved": {...}} or {"nd": {...}} overrides
    reg = [json.loads(l) for l in open(REG)]
    files = sorted(glob.glob("docs/fields/findings-*.json"))
    data = {p: json.load(open(p)) for p in files}
    where = {k: p for p, d in data.items() for k in d}
    out = collections.Counter()
    skipped = []
    for r in reg:
        if r["status"] not in ("decoded-likely", "unsettled"):
            continue
        fid = r["id"]
        if fid in SKIP:
            skipped.append((fid, SKIP[fid], r["status"]))
            continue
        p = where[fid]
        entry = data[p][fid]
        cat = decisions.get(fid, {}).get("cat") or category(r)
        d = decisions.get(fid, {})
        if r["status"] == "decoded-likely":
            if "proved" in d:
                new = {"meaning": d["proved"]["meaning"],
                       "evidence": d["proved"]["evidence"],
                       "confidence": "proved"}
                if d["proved"].get("nd"):
                    why, what = WHY[cat]
                    new["evidence"] += " Not determinable from the client: " + d["proved"]["nd"] + " " + \
                        why.split(": ", 1)[1] + " " + what
                data[p][fid] = new
                out["likely -> proved"] += 1
            else:
                text = ("Read so far, not proved: " + entry["meaning"].rstrip(".") + ". " + entry["evidence"].rstrip()
                        + " " + d.get("extra", "") )
                text = text.rstrip() + " " + nd_clause(r, cat, False)
                data[p][fid] = {"reason": re.sub(r"\s+", " ", text).strip()}
                out["likely -> not determinable"] += 1
        else:
            reason = entry["reason"].rstrip()
            if "Not determinable from the client" in reason:
                out["unsettled already stated"] += 1
                continue
            reason = reason + " " + d.get("extra", "")
            data[p][fid] = {"reason": re.sub(r"\s+", " ", reason.rstrip() + " " +
                                             nd_clause(r, cat, bool(WHAT_RE.search(reason)), reason)).strip()}
            out["unsettled -> not determinable"] += 1
    for p in files:
        indent = 2 if p.endswith("economy.json") else 1
        with open(p, "w") as f:
            f.write(json.dumps(data[p], indent=indent, ensure_ascii=False) + "\n")
    print(dict(out))
    print("skipped", len(skipped))
    for s in skipped:
        print(" ", *s)


if __name__ == "__main__":
    main()
