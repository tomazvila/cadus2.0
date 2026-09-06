#!/usr/bin/env python3
"""Apply only independently audit-clean symbolic curriculum recipes."""
from __future__ import annotations

import argparse
import tempfile
from pathlib import Path

from foundations_curriculum_patch import (
    ExemplarKey,
    ExemplarPatch,
    KpKey,
    NewExemplar,
    insert_answer_contracts,
    apply_solution_sketches,
    insert_exemplars,
    patch_exemplars,
)
from symbolic_reviewed_03 import DATA as UNIT_03
from symbolic_reviewed_04 import DATA as UNIT_04
from symbolic_reviewed_05 import DATA as UNIT_05

UNITS = {"03": UNIT_03, "04": UNIT_04, "05": UNIT_05}


def apply_unit(unit: str, curriculum_root: Path, *, write: bool) -> tuple[int, int, int, int]:
    data = UNITS[unit]
    source = curriculum_root / Path(data["path"]).relative_to("curriculum")
    temporary: Path | None = None
    if write:
        target = source
    else:
        handle = tempfile.NamedTemporaryFile(prefix=f"cadus-symbolic-{unit}-", suffix=".yaml", delete=False)
        handle.close()
        temporary = Path(handle.name)
        temporary.write_bytes(source.read_bytes())
        target = temporary
    contracts = {ExemplarKey(*key): value for key, value in data["contracts"].items()}
    additions = {
        KpKey(*key): [
            NewExemplar(
                problem=item["problem"],
                answer=item["answer"],
                solution_sketch=item["solution_sketch"],
                with_contract=item["with_contract"],
                contract_override=item["contract"] or item["contract_override"],
            )
            for item in items
        ]
        for key, items in data["additions"].items()
    }
    sketches = {ExemplarKey(*key): value for key, value in data["sketches"].items()}
    patches = {ExemplarKey(*key): ExemplarPatch(**value) for key, value in data["patches"].items()}
    _, contracted = insert_answer_contracts(target, contracts, write=True)
    _, inserted = insert_exemplars(target, additions, write=True)
    _, sketched = apply_solution_sketches(target, sketches, write=True)
    _, reviewed = patch_exemplars(target, patches, write=True)
    if temporary is not None:
        temporary.unlink()
    return len(contracted), sum(len(items) for items in additions.values()), len(sketched), len(reviewed)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--curriculum-root", type=Path, default=Path("curriculum"))
    parser.add_argument("--unit", choices=["03", "04", "05", "all"], default="all")
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    units = UNITS if args.unit == "all" else (args.unit,)
    verb = "written" if args.write else "planned"
    for unit in units:
        contracts, exemplars, sketches, reviewed = apply_unit(
            unit, args.curriculum_root, write=args.write
        )
        print(
            f"{unit}: contracts={contracts}, exemplars={exemplars}, "
            f"sketches={sketches}, reviewed={reviewed} ({verb})"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
