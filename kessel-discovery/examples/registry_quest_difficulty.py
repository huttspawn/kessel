"""Cross qstDifficulty (40000000b0e9baf2) against the difficulty tag the quest's own title shows.

usage: python3 -I registry_quest_difficulty.py <Quest.jsonl> <spice.sqlite>

Quest.jsonl is dump_objects_jsonl output for prefix qst. (lines {"fqn", "v": {low32: value}}).
Titles come from strings (id1 = 88, the title slot of str.qst, id2 = the quest object's string_id). Prints, per enum value,
the bracket tags its titles carry ([HEROIC 2+], [AREA], [WEEKLY] ...) and a few FQNs.
"""
import collections
import json
import re
import sqlite3
import sys


def main():
    db = sqlite3.connect(sys.argv[2])
    sid = dict(db.execute("select fqn, string_id from objects where kind='Quest' and is_canonical=1"))
    title = dict(db.execute("select id2, text from strings where id1=88 and fqn like 'str.qst.%'"))
    by_value = collections.defaultdict(collections.Counter)
    examples = collections.defaultdict(list)
    for line in open(sys.argv[1]):
        r = json.loads(line)
        v = r["v"].get("b0e9baf2")
        value = v["enum"] if isinstance(v, dict) else "absent"
        text = title.get(sid.get(r["fqn"]), "")
        tags = re.findall(r"\[([A-Z][A-Z ]*?)(?: \d\+?)?\]", text) or ["(no tag)"]
        for t in tags:
            by_value[value][t] += 1
        if len(examples[value]) < 4 and text:
            examples[value].append(f"{r['fqn']} = {text}")
    for value in sorted(by_value, key=str):
        print(value, sum(by_value[value].values()), by_value[value].most_common(8))
        for e in examples[value]:
            print("    ", e)


if __name__ == "__main__":
    main()
