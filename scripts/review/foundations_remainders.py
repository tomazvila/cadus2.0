#!/usr/bin/env python3
"""Verify the eight integer remainder policies from literal authored operands."""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
inventory = ROOT / "docs/reports/foundations-contract-candidates.jsonl"
rows = [json.loads(line) for line in inventory.read_text().splitlines()]
reviewed = []
for row in rows:
    if row["review_reason"] != "review_divisor":
        continue
    # Polynomial divisors require a different shape; this cohort is integer-only.
    if row["topic_id"] not in (
            "division-with-remainders", "long-division-one-digit", "long-division"):
        continue
    prompt = row["problem"].replace("{,}", "")
    match = re.fullmatch(
        r"Compute \$(\d+) \\div (\d+)\$(?:\. Give quotient and remainder\.| and verify your answer\.)",
        prompt)
    if not match:
        raise ValueError(f"unreviewed problem wording: {row['problem']}")
    dividend, divisor = map(int, match.groups())
    quotient, remainder = divmod(dividend, divisor)
    assert row["answer"] == f"{quotient} R{remainder}"
    policy = {"kind": "quotient_remainder", "divisor": divisor}
    assert row["existing_contract"] in (None, policy)
    reviewed.append({key: row[key] for key in (
        "file", "topic_id", "kp_id", "exemplar_index", "problem", "answer")} | {
            "answer_contract": policy, "dividend": dividend, "divisor": divisor,
            "quotient": quotient, "remainder": remainder,
            "verification": "dividend = divisor * quotient + remainder; 0 <= remainder < divisor",
        })
assert len(reviewed) == 8
for row in reviewed:
    print(json.dumps(row, sort_keys=True, ensure_ascii=False, separators=(",", ":")))
