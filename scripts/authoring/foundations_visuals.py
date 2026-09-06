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
        lines = path.read_text().splitlines(keepends=True)
        topic = None
        output = []
        for index, line in enumerate(lines):
            match = re.fullmatch(r'  - id: ([a-z0-9-]+)\n', line)
            if match:
                topic = match[1]
            match = re.fullmatch(r'      - id: (kp\d+)\n', line)
            output.append(line)
            if not match:
                continue
            key = f'{topic}/{match[1]}'
            if key not in entries:
                continue
            found.add(key)
            value = '        visuals: ' + json.dumps(entries[key]['visuals'], ensure_ascii=False, separators=(',', ':')) + '\n'
            if index + 1 < len(lines) and lines[index + 1].startswith('        visuals: '):
                if lines[index + 1] != value:
                    raise ValueError(f'{key}: existing visuals differ from reviewed manifest')
            else:
                output.append(value)
        text = ''.join(output)
        if text != ''.join(lines):
            changed.append(str(path))
            if write:
                path.write_text(text)
    if found != entries.keys():
        raise ValueError(f'unresolved manifest keys: {sorted(entries.keys() - found)}')
    print(json.dumps({'manifest_kps': len(entries), 'files_needing_apply': changed, 'written': write}))
    return len(changed)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true')
    args = parser.parse_args()
    count = apply(Path('.'), Path('docs/content-visuals/foundations-reference-manifest.json'), args.write)
    raise SystemExit(0 if args.write or count == 0 else 1)
