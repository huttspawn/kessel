"""Extract the HSLEnums.Enums table the client UI defines: every NAME = value the AS2 code sets.

usage: python3 -I gfx_hsl_enums.py <ui_hud.gfx> > hsl_enums.json

HSLEnums is the UI's copy of engine enums (ITM_QUALITY_*, map note types, stat ids, ...). In the
movie's AVM1 code each member is set as `Push r0, 'NAME', <number>` then SetMember, with r0 the
Enums object. Prints {name: value} for every such push whose name is UPPER_SNAKE and whose value
is a number, plus a prefix census on stderr.
"""
import collections
import json
import re
import sys

sys.path.insert(0, __file__.rsplit('/', 1)[0])
import gfx_as2_disasm as dis  # noqa: E402
import gfx_dump  # noqa: E402

NAME = re.compile(r"^'([A-Z][A-Z0-9_]+)'$")


def main():
    _, body = gfx_dump.read_body(open(sys.argv[1], 'rb').read())
    table = {}
    for code, d in gfx_dump.tags(body):
        if code not in (12, 59) or b'HSLEnums' not in d:
            continue
        for k, (_, op, arg) in enumerate(dis.disasm(d, 2 if code == 59 else 0)):
            if op != 'Push':
                continue
            parts = [p.strip() for p in arg.split(',')]
            for i in range(len(parts) - 1):
                m = NAME.match(parts[i])
                if not m:
                    continue
                try:
                    v = float(parts[i + 1])
                except ValueError:
                    continue
                table.setdefault(m.group(1), int(v) if v.is_integer() else v)
    census = collections.Counter(n.split('_')[0] for n in table)
    print(json.dumps(table, indent=1, sort_keys=True))
    print(len(table), census.most_common(40), file=sys.stderr)


if __name__ == '__main__':
    main()
