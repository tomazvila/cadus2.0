#!/usr/bin/env python3
"""Fail-closed, per-KP audit of authored Foundations practice content."""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

from template_gate_audit import GateFactsError, declined_templates, gate_results, load_gate_facts

CODES = (
    "fewer_than_four_exemplars",
    "missing_solution_sketch",
    "undecidable_authored_answer",
    "singleton_label_contract",
    "duplicate_problem_answer_family",
    "absent_pending_template_recipe",
    "generic_or_tautological_sketch",
    "pending_template_production_gate_declined",
)
NUMBER = re.compile(r"(?<![A-Za-z])[-+]?\d+(?:\.\d+)?(?:/\d+)?")
SPACE = re.compile(r"\s+")
REFLEXIVE_EQUALITY = re.compile(r"(?<![\w.])([-+]?\d+(?:\.\d+)?)\s*=\s*\1(?![\w.])")
GENERIC = (
    ("generic_order_of_operations", "evaluate grouped expressions first"),
    ("generic_appropriate_rule", "use the appropriate rule"),
    ("generic_apply_formula", "apply the formula"),
    ("generic_work_steps", "work through the steps"),
    ("generic_count_negatives", "count negatives"),
    ("generic_substitute_values", "substitute the values and simplify"),
)


class Refused(ValueError):
    """The audit input is incomplete or malformed."""


def family(text: str) -> str:
    """Normalize numeric variants while preserving operators and prose structure."""
    lowered = text.casefold().replace("\\left", "").replace("\\right", "")
    lowered = NUMBER.sub("#", lowered)
    return SPACE.sub(" ", lowered).strip(" .")


def _objects(value):
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from _objects(child)
    elif isinstance(value, list):
        for child in value:
            yield from _objects(child)


def _importer_shaped_template(row: dict) -> bool:
    """Return whether a pending row carries a complete generated recipe."""
    arguments = row.get("arguments") or row.get("body")
    if not isinstance(arguments, dict):
        return False
    for name in ("statement", "answer_expr", "solution_sketch"):
        if not isinstance(arguments.get(name), str) or not arguments[name].strip():
            return False
    if not isinstance(arguments.get("params"), dict):
        return False
    hints = arguments.get("hints")
    if not isinstance(hints, list) or not hints or any(
        not isinstance(hint, str) or not hint.strip() for hint in hints
    ):
        return False
    samples = arguments.get("samples")
    if not isinstance(samples, list) or not samples:
        return False
    return all(
        isinstance(sample, dict)
        and isinstance(sample.get("params"), dict)
        and "expected" in sample
        for sample in samples
    )


def pending_templates(root: Path) -> dict[str, list[dict]]:
    """Collect checked-in pending template recipes; reject unreadable evidence."""
    if not root.is_dir():
        raise Refused(f"content root is absent: {root}")
    files = sorted(root.rglob("*.json"))
    if not files:
        raise Refused(f"content root has no JSON evidence: {root}")
    found = defaultdict(list)
    seen = defaultdict(set)
    for path in files:
        try:
            value = json.loads(path.read_text())
        except (OSError, json.JSONDecodeError) as error:
            raise Refused(f"cannot read {path}: {error}") from error
        for row in _objects(value):
            key = row.get("kp_id")
            pending = row.get("status", "pending") == "pending"
            if (
                row.get("kind") != "template"
                or not isinstance(key, str)
                or not pending
                or not _importer_shaped_template(row)
            ):
                continue
            # Stored review exports include server-owned identity/count fields.
            # Feed their authored payload through the same importer/worker path.
            arguments = row.get("arguments") or row.get("body")
            if isinstance(arguments, dict):
                arguments = {name: value for name, value in arguments.items()
                             if name not in {"v", "topic_id", "answer_kind", "space_size"}}
                arguments.setdefault("constraints", [])
                arguments.setdefault("distractors", [])
            document = {"kp_id": key, "kind": "template", "arguments": arguments}
            fingerprint = json.dumps(document, sort_keys=True, separators=(",", ":"))
            if fingerprint not in seen[key]:
                seen[key].add(fingerprint)
                found[key].append({"document": document, "source": str(path)})
    return dict(found)


def validate_facts(value) -> list[dict]:
    if not isinstance(value, dict) or value.get("schema_version") != 1:
        raise Refused("facts must carry schema_version 1")
    if value.get("course") != "foundations" or not isinstance(value.get("kps"), list):
        raise Refused("facts must contain a Foundations KP list")
    rows = value["kps"]
    if not rows:
        raise Refused("facts contain no knowledge points")
    keys = set()
    for row in rows:
        _validate_kp(row)
        if row["kp_key"] in keys:
            raise Refused(f"duplicate KP key: {row['kp_key']}")
        keys.add(row["kp_key"])
    return rows


def _validate_kp(row) -> None:
    if not isinstance(row, dict) or not isinstance(row.get("kp_key"), str):
        raise Refused("each KP must have a string kp_key")
    exemplars = row.get("exemplars")
    if not isinstance(exemplars, list):
        raise Refused(f"{row['kp_key']}: exemplars must be a list")
    for index, exemplar in enumerate(exemplars):
        required = ("problem", "answer", "authored_answer_decidable")
        if not isinstance(exemplar, dict) or any(key not in exemplar for key in required):
            raise Refused(f"{row['kp_key']}[{index}]: incomplete exemplar facts")
        if not isinstance(exemplar["problem"], str) or not isinstance(exemplar["answer"], str):
            raise Refused(f"{row['kp_key']}[{index}]: problem and answer must be strings")
        if not isinstance(exemplar["authored_answer_decidable"], bool):
            raise Refused(f"{row['kp_key']}[{index}]: decidability must be boolean")


def _issue(code: str, **evidence) -> dict:
    return {"code": code, **evidence}


def _missing_sketches(exemplars: list[dict]) -> list[int]:
    return [
        index
        for index, row in enumerate(exemplars)
        if not isinstance(row.get("solution_sketch"), str) or not row["solution_sketch"].strip()
    ]


def _undecidable(exemplars: list[dict]) -> list[dict]:
    return [
        {"index": index, "reason": row.get("undecidable_reason") or "reason absent"}
        for index, row in enumerate(exemplars)
        if not row["authored_answer_decidable"]
    ]


def _singleton_labels(exemplars: list[dict]) -> list[int]:
    found = []
    for index, row in enumerate(exemplars):
        contract = row.get("answer_contract")
        if isinstance(contract, dict) and contract.get("kind") == "label":
            if isinstance(contract.get("options"), list) and len(contract["options"]) == 1:
                found.append(index)
    return found


def _duplicate_families(exemplars: list[dict]) -> list[dict]:
    groups = defaultdict(list)
    for index, row in enumerate(exemplars):
        groups[(family(row["problem"]), family(row["answer"]))].append(index)
    return [
        {"answer_family": key[1], "indices": indices, "problem_family": key[0]}
        for key, indices in sorted(groups.items())
        if len(indices) > 1
    ]


def _tautology(sketch: str, answer: str | None) -> bool:
    if not answer:
        return False
    clean_sketch = family(sketch).strip("$ ")
    clean_answer = family(answer).strip("$ ")
    wrappers = {
        clean_answer,
        f"answer: {clean_answer}",
        f"the answer is {clean_answer}",
        f"this gives {clean_answer}",
        f"so {clean_answer}",
    }
    return clean_sketch in wrappers


def _markers(sketch: str, answer: str | None) -> list[str]:
    lowered = SPACE.sub(" ", sketch.casefold())
    markers = [name for name, phrase in GENERIC if phrase in lowered]
    if _tautology(sketch, answer):
        markers.append("tautological_answer_restatement")
    if REFLEXIVE_EQUALITY.search(lowered):
        markers.append("tautological_reflexive_equality")
    return markers


def _generic_sketches(exemplars: list[dict], templates: list[dict]) -> list[dict]:
    found = []
    for index, row in enumerate(exemplars):
        sketch = row.get("solution_sketch")
        if not isinstance(sketch, str) or not sketch.strip():
            continue
        markers = _markers(sketch, row["answer"])
        if markers:
            found.append({"index": index, "markers": markers, "source": "authored_exemplar"})
    for index, evidence in enumerate(templates):
        row = evidence["document"]
        arguments = row.get("arguments") or row.get("body") or {}
        sketch = arguments.get("solution_sketch") if isinstance(arguments, dict) else None
        if not isinstance(sketch, str) or not sketch.strip():
            continue
        answer = arguments.get("answer_expr") or arguments.get("answer")
        markers = _markers(sketch, answer if isinstance(answer, str) else None)
        if markers:
            found.append({"index": index, "markers": markers, "source": evidence["source"]})
    return found


def audit_kp(row: dict, templates: dict[str, list[dict]], gates: dict) -> dict:
    exemplars = row["exemplars"]
    issues = []
    if len(exemplars) < 4:
        issues.append(_issue(CODES[0], actual=len(exemplars), required=4))
    for code, evidence in (
        (CODES[1], _missing_sketches(exemplars)),
        (CODES[2], _undecidable(exemplars)),
        (CODES[3], _singleton_labels(exemplars)),
        (CODES[4], _duplicate_families(exemplars)),
        (CODES[6], _generic_sketches(exemplars, templates.get(row["kp_key"], []))),
        (CODES[7], declined_templates(templates.get(row["kp_key"], []), gates)),
    ):
        if evidence:
            issues.append(_issue(code, evidence=evidence))
    if not templates.get(row["kp_key"]):
        issues.append(_issue(CODES[5]))
    return {"kp_key": row["kp_key"], "issues": issues}


def build_report(facts, templates: dict[str, list[dict]], template_gate_facts=None,
                 curriculum=Path(__file__).resolve().parents[2] / "curriculum") -> dict:
    rows = validate_facts(facts)
    try:
        gates = gate_results(facts, template_gate_facts, curriculum)
    except GateFactsError as error:
        raise Refused(str(error)) from error
    audited = [audit_kp(row, templates, gates) for row in rows]
    known = {row["kp_key"] for row in rows}
    orphans = sorted(set(templates) - known)
    counts = {code: 0 for code in CODES}
    for row in audited:
        for issue in row["issues"]:
            counts[issue["code"]] += 1
    issue_kps = sum(bool(row["issues"]) for row in audited)
    return {
        "course": "foundations",
        "issue_kps": issue_kps,
        "knowledge_points": len(audited),
        "ok": issue_kps == 0 and not orphans,
        "orphan_pending_template_keys": orphans,
        "per_code_affected_kps": counts,
        "schema_version": 1,
        "kps": audited,
    }


def load_facts(path: Path | None, curriculum: Path) -> dict:
    if path is not None:
        try:
            return json.loads(path.read_text())
        except (OSError, json.JSONDecodeError) as error:
            raise Refused(f"cannot read facts: {error}") from error
    local_cargo = Path.home() / ".cargo/bin/cargo"
    cargo = os.environ.get("CARGO") or (str(local_cargo) if local_cargo.is_file() else "cargo")
    command = [cargo, "run", "--quiet", "-p", "cadus-core", "--bin", "content_audit_facts", "--", str(curriculum)]
    try:
        result = subprocess.run(command, capture_output=True, text=True, check=False)
    except OSError as error:
        raise Refused(f"cannot start fact adapter: {error}") from error
    if result.returncode != 0:
        raise Refused(f"fact adapter failed ({result.returncode}): {result.stderr.strip()}")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise Refused(f"fact adapter emitted invalid JSON: {error}") from error


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--curriculum", type=Path, default=Path("curriculum"))
    parser.add_argument("--content-root", type=Path, default=Path("docs/content-foundations"))
    parser.add_argument("--facts", type=Path, help="precomputed facts, for deterministic tests")
    parser.add_argument("--template-gate-facts", type=Path,
                        help="precomputed worker gate facts bound to the curriculum and recipes")
    parser.add_argument("--output", type=Path, help="write JSON here; stdout when omitted")
    return parser.parse_args(argv)


def main(argv=None) -> int:
    args = parse_args(argv)
    try:
        facts = load_facts(args.facts, args.curriculum)
        templates = pending_templates(args.content_root)
        gates = load_gate_facts(args.template_gate_facts, args.curriculum, templates)
        report = build_report(facts, templates, gates, args.curriculum)
    except (Refused, GateFactsError) as error:
        print(f"Foundations content audit refused: {error}", file=sys.stderr)
        return 2
    text = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(text)
    else:
        sys.stdout.write(text)
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
