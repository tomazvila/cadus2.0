"""Scoped observations for the supplied seven-code audit; preserve audit.json as baseline.

The original full-course auditor is absent from this checkout. This report
records checks of the supplied codes and markers, not a rerun of that auditor.
Production grammar/contract verdicts come from the paired Rust verifier.
"""
import collections
import hashlib
import json
import re
from pathlib import Path

from unit06_templates import OUT

CODES = (
    "fewer_than_four_exemplars", "missing_solution_sketch", "undecidable_authored_answer",
    "duplicate_problem_answer_family", "generic_or_tautological_sketch",
    "singleton_label_contract", "absent_pending_template_recipe",
)


def family(text):
    return re.sub(r"-?\d+(?:\.\d+)?", "#", text.lower().strip().rstrip("."))


def generic(sketch):
    if "Evaluate grouped expressions first, then powers" in sketch:
        return True
    for math in re.findall(r"\$([^$]+)\$", sketch):
        sides = math.split("=")
        if len(sides) == 2 and sides[0].strip() == sides[1].strip():
            return True
    return False


def inspect(row, pending):
    exemplars = row["exemplars"]
    issues = []
    if len(exemplars) < 4:
        issues.append(CODES[0])
    if any(not e.get("solution_sketch", "").strip() for e in exemplars):
        issues.append(CODES[1])
    if any(not e["decidable"] for e in exemplars):
        issues.append(CODES[2])
    families = collections.Counter((family(e["problem"]), family(e["answer"])) for e in exemplars)
    if any(n > 1 for n in families.values()):
        issues.append(CODES[3])
    sketches = [e["solution_sketch"] for e in exemplars]
    if row["kp_id"] in pending:
        sketches.append(pending[row["kp_id"]]["body"]["solution_sketch"])
    if any(generic(s) for s in sketches):
        issues.append(CODES[4])
    if any((c := e.get("answer_contract")) and c.get("kind") == "label"
           and len(c["options"]) < 2 for e in exemplars):
        issues.append(CODES[5])
    if row["kp_id"] not in pending:
        issues.append(CODES[6])
    return {"kp_key": row["kp_id"], "issues": [{"code": c} for c in issues]}


def main():
    authored = json.loads((OUT / "authored-verification.json").read_text())
    pending = {r["kp_id"]: r for r in json.loads((OUT / "pending-review.json").read_text())}
    rows = [inspect(r, pending) for r in authored]
    counts = {c: sum(any(i["code"] == c for i in r["issues"]) for r in rows) for c in CODES}
    baseline = json.loads(Path("audit.json").read_text())
    owned = {r["kp_id"] for r in authored}
    absent = {r["kp_key"] for r in baseline["kps"] if r["kp_key"] in owned
              and any(i["code"] == CODES[6] for i in r["issues"])}
    report = dict(scope="foundations/unit06", knowledge_points=len(authored),
                  verification="Scoped checks; original full-course audit executable unavailable. Numeric-only textual families are collapsed; full pedagogical family independence is not established.",
                  baseline_sha256=hashlib.sha256(Path("audit.json").read_bytes()).hexdigest(),
                  authored_exemplars=sum(len(r["exemplars"]) for r in authored),
                  pending_templates=len(pending), newly_covered_absent_kps=len(absent & pending.keys()),
                  refreshed_existing_templates=len(pending.keys()-absent),
                  valid_distinct_instances=sum(r["valid_distinct_instances"] for r in pending.values()),
                  per_code_affected_kps=counts, kps=rows, ok=not any(counts.values()))
    (OUT / "residuals.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k:v for k,v in report.items() if k != "kps"}, indent=2))


if __name__ == "__main__":
    main()
