"""fxspec_census -- structure and semantics of every effect spec (.fxspec),
#429 criterion 7.

Input: a directory of fxspec files found by content (root <nodeWClasses>,
class _FxSpec; media_census + extract_by_hash, no dictionary). Output: the
class list, the node layout, every top-level field and list, every element
field with its value shapes, and what each list's _fxResourceName names.

  python3 -I fxspec_census.py <fxspec_dir> <out.json>
"""
import collections, json, os, re, sys
import xml.etree.ElementTree as ET

src, out = sys.argv[1], sys.argv[2]


def text(b):
    if b[:2] == b'\xff\xfe' or (len(b) > 1 and b[1] == 0):
        return b.decode('utf-16-le', 'replace').lstrip('\ufeff')
    return b.decode('utf-8', 'replace').lstrip('\ufeff')


def shape(v):
    if v is None or v == '':
        return 'empty'
    if v in ('true', 'false'):
        return 'bool'
    if re.fullmatch(r'-?\d+', v):
        return 'int'
    if re.fullmatch(r'-?\d+(\.\d+)?(e-?\d+)?', v):
        return 'float'
    if re.fullmatch(r'\(-?[\d.e-]+(,-?[\d.e-]+)+\)', v):
        return f'vec{v.count(",") + 1}'
    if re.fullmatch(r'\d+:\d\d:\d\d\.\d+', v):
        return 'duration'
    if re.fullmatch(r'[A-Z][A-Z0-9_]+', v):
        return 'ENUM'
    return 'string'


classes = collections.Counter()
files = 0; bad = []; nodes_per = collections.Counter()
top = collections.Counter()           # top-level field names on node 0
node_fields = collections.Counter()   # fields on non-zero nodes
list_elems = collections.Counter()    # list name -> element count
elem_fields = collections.defaultdict(collections.Counter)
elem_shapes = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
enum_vals = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
res_kind = collections.defaultdict(collections.Counter)
res_examples = collections.defaultdict(list)
scalar_shapes = collections.defaultdict(collections.Counter)
res_names = collections.defaultdict(set)


def res_class(v):
    lv = v.lower().replace('\\', '/')
    m = re.search(r'\.([a-z0-9_]+)$', lv)
    if m:
        return 'ext:' + m.group(1)
    if lv.startswith('play_') or lv.startswith('stop_'):
        return 'wwise-event(' + lv.split('_')[0] + ')'
    if '/' in lv:
        return 'path-noext'
    return 'name'


for fn in sorted(os.listdir(src)):
    b = open(os.path.join(src, fn), 'rb').read()
    try:
        root = ET.fromstring(text(b))
    except ET.ParseError as e:
        bad.append((fn, str(e))); continue
    files += 1
    for c in root.iter('class'):
        classes[c.text] += 1
    md = root.find('marshalData')
    ns = md.findall('node') if md is not None else []
    nodes_per[len(ns)] += 1
    for node in ns:
        for f in node.findall('f'):
            name = f.get('name')
            es = f.findall('e')
            if node.get('id') == '0':
                top[name] += 1
            else:
                node_fields[name] += 1
            if es:
                list_elems[name] += len(es)
                for e in es:
                    for g in e.findall('f'):
                        gn = g.get('name'); v = g.text or ''
                        elem_fields[name][gn] += 1
                        sh = shape(v) if not g.findall('e') else 'list'
                        elem_shapes[name][gn][sh] += 1
                        if sh == 'ENUM':
                            enum_vals[name][gn][v] += 1
                        if gn == '_fxResourceName' and v:
                            k = res_class(v)
                            res_kind[name][k] += 1
                            res_names[name].add(v)
                            if len(res_examples[(name, k)]) < 3:
                                res_examples[(name, k)].append(v)
            else:
                scalar_shapes[name][shape(f.text or '')] += 1

R = {
    'files': files, 'parse_errors': bad[:20], 'parse_error_count': len(bad),
    'classes': classes.most_common(),
    'nodes_per_file': sorted(nodes_per.items()),
    'node0_fields': top.most_common(),
    'other_node_fields': node_fields.most_common(),
    'scalar_shapes': {k: dict(v) for k, v in scalar_shapes.items()},
    'lists': {
        ln: {
            'elements': list_elems[ln],
            'fields': {fn_: [n, dict(elem_shapes[ln][fn_])] for fn_, n in elem_fields[ln].most_common()},
            'enums': {fn_: dict(c.most_common(12)) for fn_, c in enum_vals[ln].items()},
            'resource_kinds': dict(res_kind[ln]),
            'distinct_resources': len(res_names[ln]),
            'resource_examples': {k: v for (l2, k), v in res_examples.items() if l2 == ln},
        }
        for ln, _ in list_elems.most_common()
    },
}
json.dump(R, open(out, 'w'), indent=1)
json.dump({k: sorted(v) for k, v in res_names.items()}, open(out + '.resources.json', 'w'))
print('files', files, 'parse errors', len(bad), 'classes', classes.most_common(8))
print('nodes/file', sorted(nodes_per.items())[:10])
for ln, n in list_elems.most_common():
    print(f'{n:8d} {ln:34s} {dict(res_kind[ln])}')
