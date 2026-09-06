#!/usr/bin/env python3
"""Apply the content-symbolic-a lane's exemplars to
`curriculum/foundations/04-linear-graphs.yaml`. See
`apply_symbolic_expressions_equations.py` for the two-pass method this
mirrors: contracts first, then new exemplars, then any solution_sketch
backfill.

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
from symbolic_lg_apps import NEW as APPS_NEW
from symbolic_lg_equations import CONTRACTS as EQ_CONTRACTS
from symbolic_lg_equations import NEW as EQ_NEW
from symbolic_lg_interp import CONTRACTS as INTERP_CONTRACTS
from symbolic_lg_interp import NEW as INTERP_NEW
from symbolic_lg_points import CONTRACTS as POINTS_CONTRACTS
from symbolic_lg_points import NEW as POINTS_NEW
from symbolic_lg_slope import CONTRACTS as SLOPE_CONTRACTS
from symbolic_lg_slope import NEW as SLOPE_NEW
from symbolic_lg_solutions import SKETCHES
from symbolic_review_corrections import LINEAR_PATCHES as REVIEW_PATCHES

DEFAULT_PATH = "curriculum/foundations/04-linear-graphs.yaml"

CONTRACTS = {**POINTS_CONTRACTS, **SLOPE_CONTRACTS, **EQ_CONTRACTS, **INTERP_CONTRACTS}
NEW = {**POINTS_NEW, **SLOPE_NEW, **EQ_NEW, **APPS_NEW, **INTERP_NEW}


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
