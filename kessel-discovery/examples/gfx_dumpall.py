"""Disassemble every DoAction/DoInitAction block of one GFx movie into one text file.

usage: python3 -I gfx_dumpall.py <movie.gfx> <out.asm>
"""
import sys
sys.path.insert(0, __file__.rsplit('/', 1)[0])
import gfx_dump, gfx_as2_disasm as dis
path, out = sys.argv[1], sys.argv[2]
_, body = gfx_dump.read_body(open(path, 'rb').read())
with open(out, 'w') as f:
    n = 0
    for code, d in gfx_dump.tags(body):
        if code not in (12, 59):
            continue
        n += 1
        f.write(f'== block {n} tag {code} size {len(d)}\n')
        try:
            for at, name, arg in dis.disasm(d, 2 if code == 59 else 0):
                f.write(f'{at:6d} {name} {arg}\n')
        except Exception as e:
            f.write(f'ERR {e}\n')
print(n)
