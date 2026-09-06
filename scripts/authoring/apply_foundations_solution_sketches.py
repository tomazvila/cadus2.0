#!/usr/bin/env python3
"""Add every missing `solution_sketch` the pure-numeric compute family allows.

For each Foundations knowledge point whose exemplars are ALL pure-numeric
`Compute $expr$.`-shaped problems (`foundations_drafts.pure_numeric_exemplars`),
add a `solution_sketch` to any exemplar that lacks one, naming that exact
exemplar's own already-served problem and answer
(`foundations_drafts.solution_sketch_for`). No exemplar is added, removed or
reordered; no `answer` or `answer_contract` is touched; a knowledge point
with even one non-numeric exemplar is left untouched entirely.

Default is a dry run: it reports what it would change and writes nothing.
Pass `--write` to edit the curriculum files in place.
"""
import argparse
import json
import subprocess
import sys
from pathlib import Path

import foundations_drafts as fd
from foundations_curriculum_patch import ExemplarKey, Rejection, apply_solution_sketches

DEFAULT_TOPICS = Path(__file__).parent / "testdata" / "foundations_topics.json"

UNIT_FILES = {
    "arithmetic-core": "00-arithmetic-core.yaml",
    "fractions-decimals": "01-fractions-decimals.yaml",
    "integers-negatives": "02-integers-negatives.yaml",
    "expressions-equations": "03-expressions-equations.yaml",
    "linear-graphs": "04-linear-graphs.yaml",
    "systems-inequalities": "05-systems-inequalities.yaml",
    "exponents-radicals": "06-exponents-radicals.yaml",
    "polynomials-quadratics": "07-polynomials-quadratics.yaml",
    "functions-exponentials": "08-functions-exponentials.yaml",
    "rational-trig": "09-rational-trig.yaml",
    "measurement-units": "10-measurement-units.yaml",
}


def plan(topics: list[dict]) -> dict[str, dict[ExemplarKey, str]]:
    """`{unit: {ExemplarKey: sketch}}` over every topic the fixture holds."""
    by_unit: dict[str, dict[ExemplarKey, str]] = {}
    for topic in topics:
        for kp in topic["knowledge_points"]:
            missing = fd.missing_solution_sketches(kp)
            if not missing:
                continue
            for exemplar_index, sketch in missing.items():
                key = ExemplarKey(topic["id"], kp["id"], exemplar_index)
                by_unit.setdefault(topic["unit"], {})[key] = sketch
    return by_unit


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--topics", default=str(DEFAULT_TOPICS))
    parser.add_argument("--curriculum", default="curriculum/foundations")
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()

    topics = json.loads(Path(args.topics).read_text())
    by_unit = plan(topics)

    total = 0
    for unit in sorted(by_unit):
        path = Path(args.curriculum) / UNIT_FILES[unit]
        try:
            _, applied = apply_solution_sketches(path, by_unit[unit], write=args.write)
        except Rejection as error:
            print(f"REFUSED {unit}: {error}", file=sys.stderr)
            return 1
        total += len(applied)
        print(f"{unit}: {len(applied)} solution_sketch line(s) {'written' if args.write else 'planned'}")
    print(f"total: {total} ({'written' if args.write else 'dry run, nothing written'})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
