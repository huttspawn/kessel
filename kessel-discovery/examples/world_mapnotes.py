"""Parse every mapnotes.not and join it to spice.
usage: mapnotes.py <areas_dir> <spice> <out.jsonl>
Prints the census: notes, flags, template resolution, quest-task joins."""
import collections, glob, html, json, os, re, sqlite3, sys

areas, spice, out = sys.argv[1:4]
db = sqlite3.connect('file:' + spice + '?mode=ro', uri=True)
mpn = {f: (g, k) for f, g, k in db.execute(
    "select fqn, game_id, kind from objects where fqn like 'mpn.%' and is_canonical=1")}
steps = collections.defaultdict(list)  # mpn fqn -> quest steps that list it
for q, b, s, t, mn in db.execute("select quest_fqn,branch,step,task,map_notes from quest_tasks where map_notes like '%mpn.%'"):
    for n in json.loads(mn):
        if n.startswith('mpn.'):
            steps[n].append((q, b, s, t))

notes = []
for f in sorted(glob.glob(os.path.join(areas, '*', 'mapnotes.not'))):
    area = os.path.basename(os.path.dirname(f))
    t = open(f, encoding='utf-8', errors='replace').read()
    for _ in range(4):
        t = html.unescape(t)
    for m in re.finditer(r"<k>(\d+)</k><e><node>(.*?)</node></e>", t, re.S):
        nid, body = m.group(1), m.group(2)
        fld = dict(re.findall(r"<f name='(\w+)'>(.*?)</f>", body, re.S))
        x, y, z = (float(v) for v in fld['mpnPosition'].strip('()').split(','))
        rot = [float(v) for v in fld['mpnRotation'].strip('()').split(',')]
        tmpl = fld.get('mpnTemplateFQN', '').strip('\\').replace('\\', '.')
        fqn = re.sub(r'^server\.', '', tmpl).removesuffix('.mpn')
        notes.append({
            'area_dir_id': area, 'note_id': nid, 'template': fqn,
            'template_raw': fld.get('mpnTemplateFQN'),
            'x': x, 'height': y, 'z': z, 'rotation': rot,
            'parent_map': fld.get('ParentMapTag'),
            'map_tags': re.findall(r"<k>([^<]+)</k><e>([^<]*)</e>", fld.get('mpnMapTags', '')),
            'flags': {k: fld.get(k) for k in ('ShowGhosted', 'ShowOffmapArrow', 'ShowQuestBreadcrumbOnly',
                                              'DoNotBreadcrumb', 'mpnIgnoreFoW')},
            'in_spice': fqn in mpn,
            'quest_steps': len(steps.get(fqn, [])),
        })
with open(out, 'w') as o:
    for n in notes:
        o.write(json.dumps(n) + '\n')

print('notes', len(notes), 'areas', len({n['area_dir_id'] for n in notes}),
      'areas with >=1 note', len({n['area_dir_id'] for n in notes}))
print('distinct templates', len({n['template'] for n in notes}))
print('notes whose template is an mpn object in spice', sum(n['in_spice'] for n in notes))
print('notes whose template some quest step lists', sum(n['quest_steps'] > 0 for n in notes))
print('templates in spice never placed', len(set(mpn) - {n['template'] for n in notes}), 'of', len(mpn))
print('template kinds', collections.Counter(mpn[n['template']][1] for n in notes if n['in_spice']))
for k in ('ShowGhosted', 'ShowOffmapArrow', 'ShowQuestBreadcrumbOnly', 'DoNotBreadcrumb', 'mpnIgnoreFoW'):
    print(k, collections.Counter(n['flags'][k] for n in notes))
print('map tag values', collections.Counter(v for n in notes for _, v in n['map_tags']))
print('rotation nonzero', sum(any(r != 0 for r in n['rotation']) for n in notes))
print('template prefix', collections.Counter('.'.join(n['template'].split('.')[:2]) for n in notes).most_common(12))
unres = collections.Counter(n['template'] for n in notes if not n['in_spice'])
print('unresolved templates', len(unres), unres.most_common(8))
raw_prefix = collections.Counter(n['template_raw'].split('\\')[1] if n['template_raw'] else None for n in notes)
print('raw prefix', raw_prefix.most_common(5))
# quest-step listed templates: how many are placed
listed = set(steps)
print('templates listed by quest steps', len(listed), 'placed', len(listed & {n['template'] for n in notes}))
print('quest steps listing an mpn', sum(len(v) for v in steps.values()))
placed = {n['template'] for n in notes}
print('quest steps listing a placed mpn', sum(len(v) for k, v in steps.items() if k in placed))
