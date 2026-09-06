#!/usr/bin/env python3
"""Explicit, hand-verified 3rd/4th exemplars for every `integers-negatives` KP.

Every recipe is written for ONE named knowledge point and encodes THAT
knowledge point's own authored constraint literally (same sign, different
sign, subtrahend negative, one missing term, parity of negative factors, a
grouping symbol, a decimal place count, a denominator bound) —
`integers_negatives_recipes_data.RECIPES`. No exemplar's answer is a
hand-typed literal: every helper in `integers_negatives_helpers` computes the
answer from the operands with real Python arithmetic (`int`/`Fraction`), so a
transcription mistake fails an `assert` at import time instead of landing in
the curriculum. The last exemplar appended to each KP is chosen to be a
distinct family member of that KP (a sub-case its first exemplars do not
already cover: a boundary value, the opposite sign combination, an extra
term, a zero case) — the one the readiness index holds out for assessment.

`test_apply_integers_negatives_recipes.py` is the semantic table: one check
per knowledge point, re-deriving the expected answer independently (via
`foundations_compute.evaluate` for every `Compute $...$.`-shaped problem, or
a second, differently-shaped Python computation for word problems) rather
than trusting this module's own arithmetic a second time.

Two existing exemplars are undecidable under the default grammar with no
`answer_contract` (a bare relation symbol and a bare judgment word do not
parse as an expression) and are fixed in place by
`integers_negatives_contracts.CONTRACT_FIXES`. 28 more already-served
exemplars carry no `solution_sketch` at all; both are named in
`integers_negatives_contracts.py`, not here.

Default is a dry run: reports what it would change and writes nothing. Pass
`--write` to edit `curriculum/foundations/02-integers-negatives.yaml` in place.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from foundations_curriculum_patch import KpKey, Rejection, apply_solution_sketches, insert_exemplars
from integers_negatives_contracts import (
    ASCENDING_CHAIN,
    CONTRACT_FIXES,
    LABEL_LT_GT,
    LABEL_POS_NEG,
    LABEL_TRUE_FALSE,
    MISSING_SKETCHES,
    apply_contract_fixes,
)
from integers_negatives_recipes_data import RECIPES

__all__ = [
    "ASCENDING_CHAIN",
    "CONTRACT_FIXES",
    "KpKey",
    "LABEL_LT_GT",
    "LABEL_POS_NEG",
    "LABEL_TRUE_FALSE",
    "MISSING_SKETCHES",
    "RECIPES",
]

UNIT_FILE = "curriculum/foundations/02-integers-negatives.yaml"


def _options() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    specifications = (("--curriculum", {"default": UNIT_FILE}), ("--write", {"action": "store_true"}))
    for flag, options in specifications:
        parser.add_argument(flag, **options)
    return parser.parse_args()


def main() -> int:
    args = _options()
    path = Path(args.curriculum)
    verb = "written" if args.write else "planned"
    stages = [
        ("contract fixes", lambda: apply_contract_fixes(path, CONTRACT_FIXES, write=args.write)),
        ("solution sketches", lambda: apply_solution_sketches(path, MISSING_SKETCHES, write=args.write)[1]),
        ("new exemplars", lambda: insert_exemplars(path, RECIPES, write=args.write)[1]),
    ]
    for label, operation in stages:
        try:
            applied = operation()
        except Rejection as error:
            print(f"REFUSED ({label}): {error}", file=sys.stderr)
            return 1
        if label == "new exemplars":
            count = sum(len(RECIPES[key]) for key in applied)
            print(f"{len(applied)} knowledge point(s), {count} new exemplar(s) {verb}")
        else:
            print(f"{len(applied)} {label.replace(chr(32), chr(95))} item(s) {verb}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
