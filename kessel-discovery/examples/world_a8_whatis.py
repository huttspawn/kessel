"""Describe each missing area dir: files, room names, a few asset paths.
usage: a8_whatis.py <missing18> <areas_dir>"""
import os, re, sys

for m in open(sys.argv[1]).read().split():
    d = os.path.join(sys.argv[2], m)
    fs = sorted(os.listdir(d)) if os.path.isdir(d) else []
    b = open(os.path.join(d, 'area.dat'), 'rb').read() if 'area.dat' in fs else b''
    strs = [s.decode() for s in re.findall(rb'[ -~]{6,}', b)]
    paths = [s for s in strs if '\\' in s or '/' in s]
    spn = [s for s in paths if '\\spn\\' in s.lower() or 'spn' in s.lower()]
    names = [s for s in strs if '\\' not in s and '/' not in s][:12]
    print(m, 'files', len(fs), 'rooms', [f for f in fs if f.endswith('.dat') and f != 'area.dat'][:6],
          'mapnotes', 'mapnotes.not' in fs)
    print('   names', names)
    print('   paths', len(paths), paths[:4], 'spn', len(spn), spn[:3])
