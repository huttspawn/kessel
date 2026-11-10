"""wwise_orphans -- streamed .wem files that no bank's parsed sources reach:
are their ids anywhere in any bank, or in any other client file?

The streamed path is /resources/bnk2/streamed/<wem id>.wem, so a RIFF entry's
id is recovered without a dictionary by hashing candidate ids: every u32
occurring in any HIRC section is hashed into that path form and matched to
RIFF entries. Ids reached only that way (not as a parsed Sound/MusicTrack
source) are reported with the HIRC object types that hold them; RIFF entries
reached by no u32 of any HIRC are orphans.

  python3 -I wwise_orphans.py <bank_dir> <wwise.json> <census.tsv> <hash_paths>
"""
import collections, json, os, struct, subprocess, sys

bdir, wj, cen, hp = sys.argv[1:5]
W = json.load(open(wj))
riff = set()
for l in open(cen, newline=''):
    a, h, comp, cs, us, cls, head = l.rstrip('\n').split('\t')
    if cls == 'RIFF':
        riff.add(h)
holder = collections.defaultdict(set)  # u32 -> HIRC object types holding it
for fn in os.listdir(bdir):
    b = open(os.path.join(bdir, fn), 'rb').read()
    p = b.find(b'HIRC')
    if p < 0 or b[:4] != b'BKHD':
        continue
    size = struct.unpack_from('<I', b, p + 4)[0]; h = b[p + 8:p + 8 + size]
    n = struct.unpack_from('<I', h, 0)[0]; q = 4
    for _ in range(n):
        t, sz = struct.unpack_from('<BI', h, q)
        body = h[q + 5:q + 5 + sz]
        for i in range(0, len(body) - 3):
            holder[struct.unpack_from('<I', body, i)[0]].add(t)
        q += 5 + sz
parsed = {s[0] for s in W['sounds'].values()} | {int(k) for k in W['music_sources']}
ids = sorted(holder)
out = subprocess.run([hp], input='\n'.join(f'/resources/bnk2/streamed/{i}.wem' for i in ids), capture_output=True, text=True).stdout
id_of = {}
for line, i in zip(out.splitlines(), ids):
    id_of[line.split('\t')[0]] = i
reached = {h: id_of[h] for h in riff if h in id_of}
via_parse = {h for h, i in reached.items() if i in parsed}
other = {h: i for h, i in reached.items() if i not in parsed}
types = collections.Counter(t for i in other.values() for t in holder[i])
print(json.dumps({'riff': len(riff), 'reached_by_any_hirc_u32': len(reached),
                  'reached_as_parsed_source': len(via_parse),
                  'reached_only_as_raw_u32': len(other),
                  'raw_u32_holder_types': dict(types),
                  'orphans': len(riff) - len(reached)}, indent=1))
