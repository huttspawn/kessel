"""Diff decoded singletons between two extracts: which paths were added, removed or changed.

usage: python3 -I singleton_diff.py <dump_singleton binary> <old spice.sqlite> <new spice.sqlite> <out dir> <singleton fqn>...

Runs dump_singleton on each extract for every named singleton, writes both decodes to
<out dir>/<fqn>.old.json and .new.json, and prints per singleton the count of leaf paths added,
removed and changed, with up to 12 examples of each. Map entries are keyed by their 'k' so a
reordered map is not a change.
"""
import json
import subprocess
import sys


def flatten(x, path, out):
    if isinstance(x, dict):
        for k, v in x.items():
            flatten(v, f"{path}/{k}", out)
    elif isinstance(x, list):
        if x and all(isinstance(e, dict) and set(e) == {"k", "v"} for e in x):
            for e in x:
                flatten(e["v"], f"{path}[{json.dumps(e['k'])}]", out)
        else:
            for i, e in enumerate(x):
                flatten(e, f"{path}[{i}]", out)
    else:
        out[path] = x


def decode(binary, db, fqn, dest):
    text = subprocess.run([binary, db, fqn], capture_output=True, text=True, check=True).stdout
    with open(dest, "w") as f:
        f.write(text)
    return json.loads(text)


def main():
    binary, old_db, new_db, out_dir, *fqns = sys.argv[1:]
    for fqn in fqns:
        a, b = {}, {}
        flatten(decode(binary, old_db, fqn, f"{out_dir}/{fqn}.old.json"), "", a)
        flatten(decode(binary, new_db, fqn, f"{out_dir}/{fqn}.new.json"), "", b)
        added = sorted(set(b) - set(a))
        removed = sorted(set(a) - set(b))
        changed = sorted(p for p in set(a) & set(b) if a[p] != b[p])
        print(f"== {fqn}: {len(a)} -> {len(b)} leaves; +{len(added)} -{len(removed)} ~{len(changed)}")
        for p in added[:12]:
            print("  +", p, json.dumps(b[p])[:120])
        for p in removed[:12]:
            print("  -", p, json.dumps(a[p])[:120])
        for p in changed[:12]:
            print("  ~", p, json.dumps(a[p])[:60], "->", json.dumps(b[p])[:60])


if __name__ == "__main__":
    main()
