"""List the instances (tiers) of one stat amplifier.

usage: python3 -I amplifier_instances.py <statAmplifiersPrototype.json> <statAmplifierInstancesPrototype.json> <amp name>

Both inputs are dump_singleton output. statAmplifiersPrototype maps amplifier id -> {772571fd name,
77257202 stat enum, f5c7ba40 description string id}; an instance names its amplifier in 78874bec.
Prints the amplifier row and every instance that points at it with its scalar fields.
"""
import json, sys

amps, inst, name = sys.argv[1], sys.argv[2], sys.argv[3]
A = json.load(open(amps)); I = json.load(open(inst))


def rows(d):
    for v in d.values():
        if isinstance(v, list):
            for e in v:
                if isinstance(e, dict) and 'k' in e:
                    yield e


aid = None
for e in rows(A):
    if isinstance(e['v'], dict) and e['v'].get('772571fd') == name:
        aid = e['k']; print('amplifier', aid, json.dumps(e['v']))
found = 0


def walk(x):
    global found
    if isinstance(x, dict):
        if x.get('78874bec') == aid:
            found += 1
            print('  instance', json.dumps({k: v for k, v in x.items() if not isinstance(v, (list, dict))}))
        for v in x.values():
            walk(v)
    elif isinstance(x, list):
        for v in x:
            walk(v)


walk(I)
print('instances', found)
