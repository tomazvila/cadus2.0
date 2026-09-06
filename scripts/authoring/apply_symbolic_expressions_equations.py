#!/usr/bin/env python3
"""Apply the content-symbolic-a lane's exemplars to
`curriculum/foundations/03-expressions-equations.yaml`.

Two independent passes, in order:

1. **Contracts** (`add_answer_contracts`): tag existing, already-authored
   exemplars whose answer is closed-vocabulary prose (`no solution`, `all
   real numbers`, a literal written equation, a literal `a and b` count) with
   an explicit `label` policy, so the answer grammar can decide them. No
   problem, answer, or solution_sketch text changes.
2. **New exemplars** (`insert_exemplars`): append this lane's fresh,
   independently-solved exemplars after each knowledge point's last one, so
   most knowledge points reach 4 decidable exemplars (practicable AND
   assessable, D-F5).

Default is a dry run: it reports counts and writes nothing. Pass `--write`
to edit the curriculum file in place.
"""
from __future__ import annotations

import argparse
import tempfile
from pathlib import Path

from foundations_curriculum_patch import (
    KpKey,
    add_answer_contracts,
    apply_solution_sketches,
    insert_exemplars,
    patch_exemplars,
)
from symbolic_expr_eq_apps import CONTRACTS as APPS_CONTRACTS
from symbolic_expr_eq_apps import NEW as APPS_NEW
from symbolic_expr_eq_expr import NEW as EXPR_NEW
from symbolic_expr_eq_solutions import SKETCHES
from symbolic_expr_eq_solve import CONTRACTS as SOLVE_CONTRACTS
from symbolic_expr_eq_solve import NEW as SOLVE_NEW
from symbolic_review_corrections import EXPRESSION_PATCHES as REVIEW_PATCHES

DEFAULT_PATH = "curriculum/foundations/03-expressions-equations.yaml"

CONTRACTS = {**SOLVE_CONTRACTS, **APPS_CONTRACTS}
NEW = {**SOLVE_NEW, **EXPR_NEW, **APPS_NEW}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--curriculum", default=DEFAULT_PATH)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    path = Path(args.curriculum)
    temporary: Path | None = None
    if args.write:
        target = path
    else:
        handle = tempfile.NamedTemporaryFile(prefix="cadus-symbolic-", suffix=".yaml", delete=False)
        handle.close()
        temporary = Path(handle.name)
        temporary.write_bytes(path.read_bytes())
        target = temporary

    _, contracted = add_answer_contracts(target, CONTRACTS, write=True)
    exemplars = {KpKey(topic, kp): items for (topic, kp), items in NEW.items()}
    _, inserted = insert_exemplars(target, exemplars, write=True)
    _, sketched = apply_solution_sketches(target, SKETCHES, write=True)
    _, reviewed = patch_exemplars(target, REVIEW_PATCHES, write=True)

    if temporary is not None:
        temporary.unlink()
    verb = "written" if args.write else "planned"
    total = sum(len(items) for items in exemplars.values())
    print(f"contracts: {len(contracted)} existing exemplar(s) tagged ({verb})")
    print(f"exemplars: {len(inserted)} knowledge point(s), {total} new exemplar(s) ({verb})")
    print(f"sketches: {len(sketched)} existing exemplar(s) backfilled ({verb})")
    print(f"reviewed: {len(reviewed)} exact exemplar correction(s) ({verb})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
