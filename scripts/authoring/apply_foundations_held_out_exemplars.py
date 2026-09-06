#!/usr/bin/env python3
"""Add a 4th decidable, pedagogically-distinct exemplar where the family allows.

For each Foundations knowledge point of the given units whose exemplars are
ALL pure-numeric `Compute $expr$.`-shaped problems
(`foundations_drafts.pure_numeric_exemplars`), raise it to 4 decidable
exemplars (`foundations_drafts.generate_held_out_exemplars`): the readiness
audit (`crates/core/src/readiness/facts.rs`) holds out the LAST decidable
exemplar once a knowledge point has three or more, so the last of the new
ones becomes that knowledge point's held-out assessment item. A knowledge
point whose served answers already densely cover the whole band its own
author authored is left untouched (it declines rather than manufacturing an
out-of-band item).

Default is a dry run: it reports what it would change and writes nothing.
Pass `--write` to edit the curriculum files in place. Every new exemplar's
value stays inside the knowledge point's own already-authored band
(`foundations_drafts._within_kp_family`, `_operand_ceiling`) and carries its
own solution sketch, so this pass never needs a follow-up run of
`apply_foundations_solution_sketches.py`.
"""
import argparse
import json
import sys
from pathlib import Path

import foundations_drafts as fd
from apply_foundations_solution_sketches import UNIT_FILES
from foundations_curriculum_patch import KpKey, NewExemplar, Rejection, insert_exemplars

DEFAULT_TOPICS = Path(__file__).parent / "testdata" / "foundations_topics.json"
DEFAULT_UNITS = ["integers-negatives", "fractions-decimals", "exponents-radicals", "rational-trig"]


def plan(topics: list[dict], units: set[str]) -> dict[str, dict[KpKey, list[NewExemplar]]]:
    by_unit: dict[str, dict[KpKey, list[NewExemplar]]] = {}
    for topic in topics:
        if topic["unit"] not in units:
            continue
        for kp in topic["knowledge_points"]:
            new_exemplars = fd.generate_held_out_exemplars(topic, kp)
            if not new_exemplars:
                continue
            key = KpKey(topic["id"], kp["id"])
            by_unit.setdefault(topic["unit"], {})[key] = [
                NewExemplar(e.problem, e.answer, e.solution_sketch, e.with_contract)
                for e in new_exemplars
            ]
    return by_unit


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--topics", default=str(DEFAULT_TOPICS))
    parser.add_argument("--curriculum", default="curriculum/foundations")
    parser.add_argument("--unit", action="append", default=None, help="repeatable; default: the 4 units this lane already drafted teach/hint for")
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()

    units = set(args.unit) if args.unit else set(DEFAULT_UNITS)
    topics = json.loads(Path(args.topics).read_text())
    by_unit = plan(topics, units)

    total_kps = 0
    total_exemplars = 0
    for unit in sorted(by_unit):
        path = Path(args.curriculum) / UNIT_FILES[unit]
        try:
            _, applied = insert_exemplars(path, by_unit[unit], write=args.write)
        except Rejection as error:
            print(f"REFUSED {unit}: {error}", file=sys.stderr)
            return 1
        count = sum(len(by_unit[unit][key]) for key in applied)
        total_kps += len(applied)
        total_exemplars += count
        verb = "written" if args.write else "planned"
        print(f"{unit}: {len(applied)} knowledge point(s), {count} new exemplar(s) {verb}")
    print(f"total: {total_kps} knowledge points, {total_exemplars} exemplars ({'written' if args.write else 'dry run, nothing written'})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
