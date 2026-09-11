#!/usr/bin/env python3
"""Materialize production-accepted pending drafts and retire replaced local snapshots.

This operates exclusively on checked-in review files. It neither contacts a database
nor changes an approval. The authoritative production gate must run first.
"""
import json
import hashlib
from pathlib import Path

ROOT = Path('docs/content-foundations')
EVIDENCE = Path('target/unit01/evidence')


def prune_owned(path, keys):
    """Preserve every non-owned JSON object byte-for-byte in the original array."""
    text = path.read_text()
    decoder = json.JSONDecoder()
    offset = text.index('[') + 1
    chunks, removed = [], []
    while offset < len(text):
        while text[offset].isspace() or text[offset] == ',':
            offset += 1
        if text[offset] == ']':
            break
        row, end = decoder.raw_decode(text, offset)
        if row.get('kind') == 'template' and row.get('kp_id') in keys:
            removed.append(row['kp_id'])
        else:
            chunks.append(text[offset:end])
        offset = end
    result = '[\n  ' + ',\n  '.join(chunks) + '\n]\n'
    before = [r for r in json.loads(text) if not (r.get('kind') == 'template' and r.get('kp_id') in keys)]
    assert json.loads(result) == before
    path.write_text(result)
    return removed


def main():
    candidates = json.loads(Path('target/unit01/candidates.json').read_text())
    report = json.loads((EVIDENCE/'gate.json').read_text())
    assert report['candidate_sha256'] == hashlib.sha256(Path('target/unit01/candidates.json').read_bytes()).hexdigest()
    assert report['unit_sha256'] == hashlib.sha256(Path('curriculum/foundations/01-fractions-decimals.yaml').read_bytes()).hexdigest()
    by_key = {r['kp_id']: r for r in report['rows']}
    assert len(candidates) == report['checked'] == report['passed'] == 79
    passed_gate = [r for r in candidates if by_key[r['kp_id']]['passed']]
    output = ROOT/'fractions-decimals/templates'
    output.mkdir(exist_ok=True)
    for row in passed_gate:
        path = output/(row['kp_id'].replace('/', '__')+'.json')
        path.write_text(json.dumps([row], indent=2)+'\n')
    keys = {r['kp_id'] for r in passed_gate}
    for name in ('drafts.json', 'stored-review.json'):
        removed = prune_owned(ROOT/'zero-api-completion'/name, keys)
        print(name, 'replaced owned snapshots:', len(removed))
    manifest = ROOT/'fractions-decimals/manifest.json'
    value = json.loads(manifest.read_text())
    value['files'] = [p for p in value['files'] if not p.startswith('templates/')]
    value['files'] += ['templates/'+r['kp_id'].replace('/', '__')+'.json' for r in passed_gate]
    value['kinds'] = list(dict.fromkeys(value['kinds']+['template']))
    all_rows = []
    for name in value['files']:
        all_rows.extend(json.loads((manifest.parent/name).read_text()))
    value['knowledge_points'] = len({r['kp_id'] for r in all_rows})
    manifest.write_text(json.dumps(value,indent=2)+'\n')
    print('pending drafts:', len(passed_gate), 'retained reviewed recipes:', 11)


if __name__ == '__main__':
    main()
