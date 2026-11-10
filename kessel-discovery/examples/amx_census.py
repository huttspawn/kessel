"""amx_census -- decode push 3 (#429), media group: read every .amx
(magic 'AMX ') to its last byte and report counts.

Layout (little endian), proved by parsing every file to exactly its end:
  'AMX ' u8 version (1)
  records until a 0 kind byte:
    u8 kind, str8 animation name, str8 network folder ('humanoid\\bmnnew\\')
    kind 3 (and 1) are followed by a u32 (always 0 so far); kind 2 is not
  u32 skeleton count (1)
  per skeleton: u32 bone count, then per bone:
    str8 bone name, f32[4] rotation (x y z w), f32[3] translation
str8 = u8 length + bytes, no terminator.

The bone list names the anonymous tracks of the JAWB animations of the same
network, in track order (the JAWB track count equals the bone count).
The animation name + folder names a .jba: /resources/anim/<folder><name>.jba;
the census says how many of those hash to an archive entry (no dictionary).

Usage: python3 -I amx_census.py <dir of AMX files> [census.tsv]
"""
import collections
import os
import struct
import sys
import importlib.util

here = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location('ap', os.path.join(here, 'appearance_paths.py'))
ap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ap)


class Bad(Exception):
    pass


def str8(d, o):
    n = d[o]
    return d[o + 1:o + 1 + n].decode('latin-1'), o + 1 + n


def parse(d):
    if d[:4] != b'AMX ':
        raise Bad('magic')
    ver = d[4]
    o = 5
    recs = []
    tails = collections.Counter()
    while True:
        kind = d[o]
        o += 1
        if kind == 0:
            break
        name, o = str8(d, o)
        folder, o = str8(d, o)
        extra = None
        if kind in (1, 3):
            extra = struct.unpack_from('<I', d, o)[0]
            o += 4
        elif kind != 2:
            raise Bad(f'kind {kind}')
        recs.append((kind, name, folder, extra))
        tails[(kind, extra)] += 1
    nskel = struct.unpack_from('<I', d, o)[0]
    o += 4
    skels = []
    for _ in range(nskel):
        nb = struct.unpack_from('<I', d, o)[0]
        o += 4
        bones = []
        for _ in range(nb):
            name, o = str8(d, o)
            q = struct.unpack_from('<7f', d, o)
            o += 28
            bones.append((name, q))
        skels.append(bones)
    return ver, recs, skels, o, tails


def main():
    src = sys.argv[1]
    present = set()
    if len(sys.argv) > 2:
        for line in open(sys.argv[2]):
            p = line.split('\t')
            try:
                present.add(int(p[1], 16))
            except (IndexError, ValueError):
                pass
    c = collections.Counter()
    kinds = collections.Counter()
    extras = collections.Counter()
    bonecounts = collections.Counter()
    folders = collections.Counter()
    orders = collections.defaultdict(collections.Counter)
    jba = set()
    for fn in sorted(os.listdir(src)):
        d = open(os.path.join(src, fn), 'rb').read()
        c['files'] += 1
        try:
            ver, recs, skels, end, tails = parse(d)
        except (Bad, IndexError, struct.error) as e:
            c[f'x unparsed: {e}'] += 1
            continue
        if end != len(d):
            c['x trailing bytes'] += 1
            continue
        c['parsed to the last byte'] += 1
        c[f'version {ver}'] += 1
        c[f'skeleton count {len(skels)}'] += 1
        for k, extra in tails:
            extras[(k, extra)] += tails[(k, extra)]
        for kind, name, folder, extra in recs:
            kinds[kind] += 1
            if extra is not None:
                c['pose index < pose-table count' if extra < len(skels) else 'x pose index >= pose-table count'] += 1
            if kind == 2:
                c['kind 2 name prefix ' + name.split('_')[0]] += 1
            folders[folder.rstrip('\\').split('\\')[0]] += 1
            f = folder.replace('\\', '/')
            if not f.endswith('/'):
                f += '/'
            jba.add(f'/resources/anim/{f}{name}.jba')
        for bones in skels:
            bonecounts[len(bones)] += 1
            orders[len(bones)][tuple(b[0] for b in bones)] += 1
    print(dict(c))
    print('record kinds', dict(kinds))
    print('kind/extra u32', {f'{k}:{e}': n for (k, e), n in extras.most_common(10)})
    print('bone counts', dict(bonecounts.most_common(15)))
    print('top network roots', dict(folders.most_common(15)))
    for n, ords in sorted(orders.items(), key=lambda kv: -sum(kv[1].values()))[:4]:
        print(f'bone count {n}: {len(ords)} distinct name orders; most common:', list(ords.most_common(1)[0][0])[:8], '...')
    if present:
        hit = sum(ap.hashlittle2(p.encode()) in present for p in jba)
        print(f'jba paths named by AMX: {len(jba)}, hash to an archive entry: {hit}')
        with open(os.path.join(os.path.dirname(src.rstrip('/')), 'amx_jba_paths.txt'), 'w') as fh:
            fh.write('\n'.join(sorted(jba)) + '\n')


if __name__ == '__main__':
    main()
