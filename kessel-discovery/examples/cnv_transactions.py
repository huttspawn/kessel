"""Debit (946e6bf1) and credit (946e6bf2) transactions on conversation nodes: what each holds
(9a7ba580 items, 9a7ba581 influence, 946e6bfb credits), with the node's condition and 946e6bfc.
Usage: tx.py nodes.jsonl > tx.txt; writes tx_rows.jsonl"""
import json, sys, collections

C = collections.Counter()
rows = open('tx_rows.jsonl', 'w')
for line in open(sys.argv[1]):
    d = json.loads(line)
    f = d['f']
    rec = {}
    for side, fid in (('debit', '946e6bf1'), ('credit', '946e6bf2')):
        t = f.get(fid) or {}
        items = [(x['k'], x['v']) for x in t.get('9a7ba580') or []]
        infl = [(x['k'], x['v']) for x in t.get('9a7ba581') or []]
        cr = t.get('946e6bfb')
        if items or infl or cr is not None:
            rec[side] = {'items': items, 'influence': infl, 'credits': cr}
            for kind, v in (('items', items), ('influence', infl), ('credits', cr)):
                if v:
                    C[f'{side} {kind}'] += 1
            C[f'{side} keys ' + ','.join(sorted(t.keys()))] += 1
    if rec:
        C['nodes with a transaction'] += 1
        C['with 946e6bfc'] += '946e6bfc' in f
        C['with condition'] += '6584d6d0' in f
        rec.update(cnv=d['fqn'], node=d['node'], cond=f.get('6584d6d0'), x6bfc=f.get('946e6bfc'),
                   expr=f.get('bc126752'), quest=f.get('b21f4106'))
        rows.write(json.dumps(rec) + '\n')
for k, n in sorted(C.items()):
    print(k, n)
