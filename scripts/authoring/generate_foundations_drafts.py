#!/usr/bin/env python3
"""Write pending-only teach/hint_ladder drafts for the pure-numeric compute family.

Reads the trimmed curriculum fixture (`dump_foundations_topics.py`), skips
every topic already named in `--skip-unit` (arithmetic-core already holds
hand-authored drafts from `framework/content-batch`), classifies every
remaining knowledge point with `foundations_drafts.classify_kp`, and writes
one `docs/content-foundations/<unit>/manifest.json` plus one `teach.json` and
one `hints.json` per unit that gains at least one draft. Every draft this
script writes still needs `scripts/authoring/import_local_drafts.py` and the
production `cadus-worker author` gate before any row reaches a database, and
that import only ever stores `pending` rows.

Also writes `--residual-report`, one line per Foundations knowledge point
this script did NOT draft, with the reason, so the count of what remains is
exact and auditable.
"""
import argparse
import json
import random
import sys
from pathlib import Path

import foundations_compute as fc
import foundations_drafts as fd

DEFAULT_TOPICS = Path(__file__).parent / "testdata" / "foundations_topics.json"


def reason_for_skip(topic: dict, kp: dict) -> str:
    for exemplar in kp["exemplars"]:
        match = fc.match_problem(exemplar["problem"])
        if not match:
            return "not every exemplar is a Compute/Calculate/Evaluate/Simplify $expr$. problem"
        if not fc.is_pure_numeric(match.group(2)):
            return "an exemplar expression names a letter outside the whitelisted LaTeX commands"
        try:
            fc.parse_answer_text(exemplar["answer"])
        except (ValueError, ZeroDivisionError):
            return "an exemplar answer is not a plain number, fraction or decimal"
    return "no fresh same-shape operand set was found (same_shape_new_operands exhausted its draws)"


def generate(topics: list[dict], skip_units: set[str]) -> tuple[dict, dict, list[dict]]:
    """`(teach_by_unit, hints_by_unit, residuals)` over every topic not in `skip_units`."""
    teach_by_unit: dict[str, list[dict]] = {}
    hints_by_unit: dict[str, list[dict]] = {}
    residuals: list[dict] = []
    for topic in topics:
        if topic["unit"] in skip_units:
            continue
        for kp in topic["knowledge_points"]:
            rng = random.Random(f"{topic['id']}/{kp['id']}")
            candidate = fd.classify_kp(topic, kp, rng)
            if candidate is None:
                residuals.append(
                    {
                        "kp_id": f"{topic['id']}/{kp['id']}",
                        "unit": topic["unit"],
                        "reason": reason_for_skip(topic, kp),
                    }
                )
                continue
            teach_by_unit.setdefault(topic["unit"], []).append(fd.teach_draft(candidate))
            hints_by_unit.setdefault(topic["unit"], []).append(fd.hint_draft(candidate))
    return teach_by_unit, hints_by_unit, residuals


def write_unit(out_dir: Path, unit: str, teach: list[dict], hints: list[dict]) -> None:
    unit_dir = out_dir / unit
    unit_dir.mkdir(parents=True, exist_ok=True)
    (unit_dir / "teach.json").write_text(json.dumps(teach, indent=2, sort_keys=True) + "\n")
    (unit_dir / "hints.json").write_text(json.dumps(hints, indent=2, sort_keys=True) + "\n")
    manifest = {
        "kinds": ["teach", "hint_ladder"],
        "knowledge_points": len(teach),
        "files": ["teach.json", "hints.json"],
    }
    (unit_dir / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--topics", default=str(DEFAULT_TOPICS))
    parser.add_argument("--out", required=True, help="docs/content-foundations")
    parser.add_argument(
        "--skip-unit",
        action="append",
        default=["arithmetic-core"],
        help="a unit id to leave untouched (repeatable); default: arithmetic-core",
    )
    parser.add_argument("--residual-report", default=None)
    args = parser.parse_args()

    topics = json.loads(Path(args.topics).read_text())
    teach_by_unit, hints_by_unit, residuals = generate(topics, set(args.skip_unit))

    out_dir = Path(args.out)
    skip_units = set(args.skip_unit)
    for stale in sorted(out_dir.iterdir()) if out_dir.is_dir() else []:
        if stale.is_dir() and stale.name not in skip_units and stale.name not in teach_by_unit:
            print(f"removing {stale}: it no longer classifies any knowledge point")
            for child in stale.iterdir():
                child.unlink()
            stale.rmdir()
    for unit in sorted(teach_by_unit):
        write_unit(out_dir, unit, teach_by_unit[unit], hints_by_unit[unit])

    total = sum(len(rows) for rows in teach_by_unit.values())
    print(f"generated {total} teach+hint_ladder pairs across {len(teach_by_unit)} units")
    for unit in sorted(teach_by_unit):
        print(f"  {unit}: {len(teach_by_unit[unit])} knowledge points")
    print(f"residual (not drafted): {len(residuals)} knowledge points")

    if args.residual_report:
        Path(args.residual_report).write_text(
            json.dumps(residuals, indent=2, sort_keys=True) + "\n"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
