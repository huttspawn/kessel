"""Check effPostfixElements (4000004c8cbfea50) as a postfix expression over the sub-effect's conditions.

usage: python3 -I registry_postfix_check.py <abl_all.jsonl>

abl_all.jsonl holds every ability row (canonical and variant) with full 16-hex field ids. In each
sub-effect, an element with no type (e71e2f92) pushes condition id e71f8f23, which must be a key
of the same sub-effect's effConditions map (40000040f1040f75); an element with a type is an
operator whose id indexes effLogicOp. Counts lists that evaluate to exactly one value with
binary operators, operand ids that resolve, and the operator (type, id) pairs seen.
"""
import collections
import json
import sys

LIST, CONDS, TYPE, ID = "4000004c8cbfea50", "40000040f1040f75", "40000040e71e2f92", "40000040e71f8f23"


def main():
    stats = collections.Counter()
    ops = collections.Counter()
    lengths = collections.Counter()

    def check(sub):
        elems = sub[LIST]
        keys = {e["k"] for e in sub.get(CONDS) or []}
        depth = 0
        ok = True
        for e in elems:
            t = e.get(TYPE)
            if t is None:
                stats["operand"] += 1
                stats["operand resolves" if e.get(ID) in keys else "operand missing"] += 1
                depth += 1
            else:
                kind = t.get("enum") if isinstance(t, dict) else t
                ops[(kind, e.get(ID))] += 1
                if depth < 2:
                    ok = False
                depth -= 1
        stats["lists"] += 1
        stats["valid" if ok and depth == 1 else "invalid"] += 1
        stats["conditions in map"] += len(keys)
        stats["conditions used"] += len({e.get(ID) for e in elems if e.get(TYPE) is None} & keys)
        lengths[len(elems)] += 1

    def walk(v):
        if isinstance(v, dict):
            if v.get(LIST):
                check(v)
            for x in v.values():
                walk(x)
        elif isinstance(v, list):
            for x in v:
                walk(x)

    for line in open(sys.argv[1]):
        if LIST in line:
            walk(json.loads(line)["v"])
    print(dict(stats))
    print("operators (type, id):", ops.most_common())
    print("list lengths:", sorted(lengths.items()))


if __name__ == "__main__":
    main()
