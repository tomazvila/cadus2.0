#!/usr/bin/env python3
"""Generate unit01 worker draft arguments for production-gate review, without a DB."""
import json
from pathlib import Path
import unit01_templates_fractions as fractions
import unit01_templates_decimals as decimals
import unit01_templates_percent_ratio as percent_ratio
from unit01_templates_reviewed_repairs import REPAIRS

# These current-head recipes already pass review and remain in the shared
# zero-API completion packet.  Unit01 only fills absent or audit-failing keys.
RETAINED = {
    'equivalent-fractions/kp2', 'equivalent-fractions/kp3',
    'adding-subtracting-like-fractions/kp1',
    'adding-subtracting-like-fractions/kp2',
    'adding-subtracting-like-fractions/kp3',
    'adding-subtracting-fractions/kp1',
    'adding-subtracting-fractions/kp2',
    'adding-subtracting-fractions/kp3',
    'multiplying-fractions/kp1', 'multiplying-fractions/kp2',
    'multiplying-fractions/kp3',
}


def drafts():
    all_rows = list(fractions.recipes()) + list(decimals.recipes()) + list(percent_ratio.recipes())
    all_rows = [REPAIRS.get(row['kp_id'], row) for row in all_rows]
    assert len(all_rows) == len({r['kp_id'] for r in all_rows}) == 90
    rows = [row for row in all_rows if row['kp_id'] not in RETAINED]
    assert len(rows) == len({r['kp_id'] for r in rows}) == 79
    return rows


if __name__ == '__main__':
    target = Path('target/unit01/candidates.json')
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(json.dumps(drafts(), indent=2) + '\n')
    print(f'{len(drafts())} real template candidates written to {target}')
