"""Index every Scaleform movie of a client: the client getters its ActionScript calls and the
enums, stats and string keys it names.

usage: python3 -I gfx_index.py <dump dir> <dump_archive tsv> <out index.json> docs/fields/names.json kessel/resources/gom_enums.json

The dump dir and tsv come from dump_archive.rs run on <install>/swtor/retailclient/main_gfx_1.tor.
For every AS2 block (DoAction, DoInitAction) the class it defines is read from the
'_global.<pkg>...<Class>.prototype' path at the head of its constant pool. Calls into the client
are the ExternalInterface.call sites: an AVM1 Push whose last three values are
'<name>', <arg count + 1>, 'flash', followed by GetMember external / ExternalInterface / call.
The index records, per getter, each movie and class that calls it. It also records every
constant naming an HSL enum member (ITM_QUALITY_*, STAT_*, ...), and every 'str.*' or GUI_* key.
"""
import collections
import json
import re
import sys

sys.path.insert(0, __file__.rsplit('/', 1)[0])
import gfx_as2_disasm as dis  # noqa: E402
import gfx_dump  # noqa: E402

ENUMISH = re.compile(r'^[A-Z][A-Z0-9]*(_[A-Z0-9]+)+$')
STRKEY = re.compile(r'^(str\.|GUI_|gui_)')


def class_of(pool):
    if not pool or pool[0] not in ('_global', 'as', 'mx'):
        return None
    path = []
    for w in pool[1:] if pool[0] == '_global' else pool:
        if w in ('Object', 'prototype'):
            continue
        if w and (w[0].isupper() or w in ('as', 'mx', 'UI', 'flash')) and len(path) < 8:
            path.append(w)
        else:
            break
    return '.'.join(path) or None


def main():
    dump_dir, tsv, out_path, names_json, enums_json = sys.argv[1:]
    all_words = collections.defaultdict(set)
    index = {'getters': collections.defaultdict(lambda: collections.defaultdict(set)),
             'enums': collections.defaultdict(set), 'string_keys': collections.defaultdict(set),
             'movies': {}}
    for line in open(tsv):
        h, size, magic, path = (line.rstrip('\n').split('\t') + [''])[:4]
        if not magic.startswith(('CFX', 'GFX', 'CWS', 'FWS')):
            continue
        name = path.rsplit('/', 1)[-1] or h
        try:
            _, body = gfx_dump.read_body(open(f'{dump_dir}/{h}.bin', 'rb').read())
        except Exception as e:  # noqa: BLE001
            index['movies'][name] = {'error': str(e)}
            continue
        classes, calls = set(), 0
        for code, d in gfx_dump.tags(body):
            if code not in (12, 59):
                continue
            start = 2 if code == 59 else 0
            try:
                ops = dis.disasm(d, start)
            except Exception:  # noqa: BLE001
                continue
            # constant pool strings
            i = start
            words = []
            while i < len(d):
                o = d[i]
                if o >= 0x80:
                    if i + 3 > len(d):
                        break
                    ln = d[i + 1] | (d[i + 2] << 8)
                    if o == 0x88:
                        cnt = d[i + 3] | (d[i + 4] << 8)
                        words = [w.decode('latin-1') for w in d[i + 5:i + 3 + ln].split(b'\0')[:cnt]]
                        break
                    i += 3 + ln
                else:
                    i += 1
            cls = class_of(words)
            if cls:
                classes.add(cls)
            for w in words:
                all_words[w].add(name)
                if ENUMISH.match(w):
                    index['enums'][w].add(name)
                if STRKEY.match(w):
                    index['string_keys'][w].add(name)
            for k, (_, op, arg) in enumerate(ops):
                if op != 'Push' or not arg.endswith("'flash'"):
                    continue
                parts = [p.strip() for p in arg.split(',')]
                if len(parts) < 3 or not parts[-3].startswith("'"):
                    continue
                window = ' '.join(a for _, _, a in ops[k + 1:k + 8])
                if "'ExternalInterface'" in window and "'call'" in window:
                    getter = parts[-3].strip("'")
                    try:
                        nargs = int(parts[-2]) - 1
                    except ValueError:
                        nargs = None
                    index['getters'][getter][name].add(f'{cls or "?"} args={nargs}')
                    calls += 1
        index['movies'][name] = {'hash': h, 'classes': sorted(classes), 'external_calls': calls}
    # GOM names the UI code spells out: hash-proved field/class/enum names and enum members.
    gom = {r['name']: r.get('kind') for r in json.load(open(names_json))['records'] if r.get('name')}
    members = {}
    for e in json.load(open(enums_json)):
        for m in e.get('members', []):
            members[m] = e.get('name')
    gom_hits = {w: {'kind': gom[w], 'movies': sorted(ms)} for w, ms in all_words.items() if w in gom}
    member_hits = {w: {'enum': members[w], 'movies': sorted(ms)} for w, ms in all_words.items() if w in members}
    out = {
        'gom_names_used': dict(sorted(gom_hits.items())),
        'gom_enum_members_used': dict(sorted(member_hits.items())),
        'getters': {g: {m: sorted(v) for m, v in ms.items()} for g, ms in sorted(index['getters'].items())},
        'enums': {e: sorted(v) for e, v in sorted(index['enums'].items())},
        'string_keys': {s: sorted(v) for s, v in sorted(index['string_keys'].items())},
        'movies': index['movies'],
    }
    json.dump(out, open(out_path, 'w'), indent=1, sort_keys=True)
    print('movies', len(out['movies']), 'getters', len(out['getters']), 'enums', len(out['enums']), 'string keys', len(out['string_keys']))


if __name__ == '__main__':
    main()
