#!/usr/bin/env python3
"""Apply a reviewed visual manifest without rewriting unrelated curriculum YAML.
Run from the repository root. Default is a read-only consistency check.
"""
import argparse
import json
import re
from pathlib import Path


def apply(root, manifest, write=False):
    entries = {row['kp_id']: row for row in json.loads(manifest.read_text())}
    found = set()
    changed = []
    for path in sorted((root / 'curriculum/foundations').glob('*.yaml')):
        text, file_found, needs_write = patched_file(path, entries)
        found.update(file_found)
        if needs_write:
            changed.append(str(path))
            if write:
                path.write_text(text)
    if found != entries.keys():
        raise ValueError(f'unresolved manifest keys: {sorted(entries.keys() - found)}')
    print(json.dumps({'manifest_kps': len(entries), 'files_needing_apply': changed, 'written': write}))
    return len(changed)


def patched_file(path, entries):
    """Return one curriculum file with its reviewed visual rows inserted."""
    lines = path.read_text().splitlines(keepends=True)
    topic = None
    output = []
    found = set()
    for index, line in enumerate(lines):
        topic_match = re.fullmatch(r'  - id: ([a-z0-9-]+)\n', line)
        topic = topic_match[1] if topic_match else topic
        kp_match = re.fullmatch(r'      - id: (kp\d+)\n', line)
        output.append(line)
        if not kp_match:
            continue
        key = f'{topic}/{kp_match[1]}'
        if key not in entries:
            continue
        found.add(key)
        append_visual(lines, output, index, key, entries[key]['visuals'])
    text = ''.join(output)
    return text, found, text != ''.join(lines)


def append_visual(lines, output, index, key, visuals):
    """Validate an existing visual row or append the reviewed missing row."""
    value = '        visuals: ' + json.dumps(
        visuals, ensure_ascii=False, separators=(',', ':')
    ) + '\n'
    existing = index + 1 < len(lines) and lines[index + 1].startswith('        visuals: ')
    if existing and lines[index + 1] != value:
        raise ValueError(f'{key}: existing visuals differ from reviewed manifest')
    if not existing:
        output.append(value)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true')
    args = parser.parse_args()
    count = apply(Path('.'), Path('docs/content-visuals/foundations-reference-manifest.json'), args.write)
    raise SystemExit(0 if args.write or count == 0 else 1)
