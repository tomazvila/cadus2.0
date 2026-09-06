"""Contract fixes and missing-sketch data for `integers-negatives`.

Two existing exemplars are undecidable under the default grammar with no
`answer_contract` (a bare relation symbol and a bare judgment word do not
parse as an expression) and are fixed in place by `CONTRACT_FIXES` via
`apply_contract_fixes` — see `docs/reference/checker-1.0-spec.md` and the
module docstring of `foundations_curriculum_patch.py` for why a `multi-step`
topic's answer needs an explicit contract to be graded deterministically at
all. `apply_contract_fixes` inserts one `answer_contract:` line right after
each named exemplar's `answer:` line; `insert_exemplars`/
`apply_solution_sketches` do not do this (they only append a trailing field
or a new block), so this is a small dedicated patch instead of an extension
to the shared module.

`MISSING_SKETCHES` names every practice exemplar this file already served
with no `solution_sketch` at all, found by scanning every KP this unit
raises to 4+ decidable exemplars: `readiness::KpFacts::solutions` requires
EVERY non-held-out decidable exemplar to carry one, and these KPs only reach
that many decidable exemplars once this unit's own new rows land beside
them. Every sketch names only that exemplar's own already-served problem and
answer, the same convention `apply_foundations_solution_sketches.py` uses.
"""
from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from foundations_curriculum_patch import ExemplarKey, Rejection

UNIT_FILE = "curriculum/foundations/02-integers-negatives.yaml"

LABEL_TRUE_FALSE = '{"kind":"label","options":[["true"],["false"]]}'
LABEL_LT_GT = '{"kind":"label","options":[["<"],[">"]]}'
LABEL_POS_NEG = '{"kind":"label","options":[["positive"],["negative"]]}'
ASCENDING_CHAIN = "{kind: ascending_chain}"
EXACT = "{kind: exact}"


#: 27 practice exemplars, across 21 knowledge points, that this file already
#: served with no `solution_sketch` at all — found by scanning every KP this
#: script raises to 4+ decidable exemplars (`readiness::KpFacts::solutions`
#: requires EVERY non-held-out decidable exemplar to carry one, and these KPs
#: only reach that many decidable exemplars once this script's own new rows
#: land beside them). Every sketch here names only that exemplar's own
#: already-served problem and answer, the same convention
#: `apply_foundations_solution_sketches.py` uses for the pure-numeric family.
MISSING_SKETCHES: dict[ExemplarKey, str] = {
    ExemplarKey("plotting-integers", "kp1", 0):
        "Below zero is negative: $5$ degrees below zero is $-5$.",
    ExemplarKey("plotting-integers", "kp1", 1):
        "A loss is the opposite of a gain: a loss of $8$ metres is $-8$.",
    ExemplarKey("plotting-integers", "kp2", 1):
        "Right of $0$ is positive; $6$ units right is $6$.",
    ExemplarKey("opposites-of-integers", "kp1", 1):
        "The opposite of $12$ is $-12$: the same distance from $0$, on the other side.",
    ExemplarKey("opposites-of-integers", "kp1", 2):
        "The opposite of $0$ is $0$ itself; $0$ is its own mirror image.",
    ExemplarKey("number-line-integers", "kp1", 1):
        "Placed on the number line, the order is $-4, -1, 0, 2$.",
    ExemplarKey("comparing-integers", "kp1", 1):
        "$3$ is right of $-7$ on the number line, so $3 > -7$.",
    ExemplarKey("comparing-integers", "kp2", 1):
        "On the number line the order least to greatest is $-4 < -1 < 3$.",
    ExemplarKey("absolute-value", "kp1", 0):
        "The distance of $-6$ from $0$ is $6$.",
    ExemplarKey("absolute-value", "kp1", 1):
        "The distance of $9$ from $0$ is $9$.",
    ExemplarKey("absolute-value", "kp2", 0):
        "$|-4| = 4$, $|3| = 3$; $4 + 3 = 7$.",
    ExemplarKey("absolute-value", "kp2", 1):
        "$|-10| = 10$, $|-4| = 4$; $10 - 4 = 6$.",
    ExemplarKey("adding-integers", "kp1", 1):
        "Same signs: $7 + 2 = 9$, keep the sign: $-9$.",
    ExemplarKey("adding-integers", "kp2", 1):
        "Different signs: $9 - 4 = 5$, keep the sign of the larger absolute value: $5$.",
    ExemplarKey("integer-addition-subtraction", "kp1", 0):
        "$4 + (-9) = -5$.",
    ExemplarKey("integer-addition-subtraction", "kp2", 1):
        "$10 - 15 = -5$; then $-5 + (-3) = -8$.",
    ExemplarKey("multiplying-integers", "kp1", 1):
        "One negative factor: the product is negative; $7 \\times 6 = 42$.",
    ExemplarKey("multiplying-integers", "kp2", 1):
        "Two negative factors: the product is positive; $8 \\times 4 = 32$.",
    ExemplarKey("dividing-integers", "kp1", 1):
        "One negative: the quotient is negative; $35 \\div 7 = 5$, so $-5$.",
    ExemplarKey("dividing-integers", "kp2", 0):
        "Two negatives: the quotient is positive; $18 \\div 3 = 6$.",
    ExemplarKey("dividing-integers", "kp2", 1):
        "Two negatives: the quotient is positive; $48 \\div 6 = 8$.",
    ExemplarKey("signed-decimal-operations", "kp1", 0):
        "Different signs: $2.5 - 1.5 = 1$, keep the sign of the larger absolute value: $-1$.",
    ExemplarKey("signed-decimal-operations", "kp1", 2):
        "$4.3 + (-7.5) = -3.2$.",
    ExemplarKey("signed-decimal-operations", "kp2", 0):
        "One negative factor: the product is negative; $0.5 \\times 6 = 3$.",
    ExemplarKey("negative-fractions-decimals", "kp1", 1):
        "One negative factor: the quotient is negative; $\\frac{1}{2} \\div \\frac{1}{4} = 2$.",
    ExemplarKey("exponent-notation", "kp1", 0):
        "Three equal factors of $4$: $4^3$.",
    ExemplarKey("exponent-notation", "kp2", 0):
        "$5 \\times 5 = 25$.",
    ExemplarKey("exponent-notation", "kp2", 1):
        "$3 \\times 3 \\times 3 = 27$.",
}


# ---------------------------------------------------------------------------
# Two undecidable rows already in the file (no `answer_contract`, and their
# bare answer text does not parse as an expression under the default
# grammar): a fill-in-the-blank relation symbol, and a positive/negative
# judgment word. Both need an explicit `answer_contract` line added to the
# EXISTING exemplar block, which `insert_exemplars`/`apply_solution_sketches`
# do not do (they only append a trailing field or a new block), so these three
# lines are applied by a small dedicated patch instead of the shared module.
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ContractFix:
    topic_id: str
    kp_id: str
    exemplar_index: int
    answer_text: str
    contract_line: str


CONTRACT_FIXES: list[ContractFix] = [
    ContractFix("comparing-integers", "kp1", 0, "<", LABEL_LT_GT),
    ContractFix("comparing-integers", "kp1", 1, ">", LABEL_LT_GT),
    ContractFix("comparing-integers", "kp2", 1, "-4 < -1 < 3", ASCENDING_CHAIN),
    ContractFix("integer-multiplication-division", "kp3", 0, "negative", LABEL_POS_NEG),
]


def apply_contract_fixes(path: Path, fixes: list[ContractFix], *, write: bool) -> list[ContractFix]:
    """Insert one `answer_contract:` line right after each named exemplar's `answer:` line.

    Refuses the whole file, writing nothing, if any fix's own `(topic, kp,
    exemplar_index, answer_text)` is not found exactly, or if that exemplar
    already carries an `answer_contract` line (never overwritten).
    """
    lines = path.read_text().splitlines(keepends=True)
    topic_id: str | None = None
    kp_id: str | None = None
    exemplar_index = -1
    remaining = {(f.topic_id, f.kp_id, f.exemplar_index): f for f in fixes}
    output: list[str] = []
    applied: list[ContractFix] = []
    index = 0
    while index < len(lines):
        line = lines[index]
        if line.startswith("  - id: "):
            topic_id, kp_id, exemplar_index = line.strip()[len("- id: "):], None, -1
        elif line.startswith("      - id: kp"):
            kp_id = line.strip()[len("- id: "):]
            exemplar_index = -1
        elif line.lstrip().startswith("- problem: "):
            exemplar_index += 1
        output.append(line)
        index += 1
        key = (topic_id, kp_id, exemplar_index)
        if key not in remaining:
            continue
        fix = remaining[key]
        # The next line is this exemplar's `answer:` field (every exemplar in
        # this file writes `problem` then `answer` back to back).
        answer_line = lines[index]
        if fix.answer_text not in answer_line:
            raise Rejection(f"{key}: expected answer {fix.answer_text!r} in {answer_line!r}")
        output.append(answer_line)
        index += 1
        if lines[index].lstrip().startswith("answer_contract:"):
            raise Rejection(f"{key}: already carries an answer_contract")
        indent = " " * (len(answer_line) - len(answer_line.lstrip()))
        output.append(f"{indent}answer_contract: {fix.contract_line}\n")
        applied.append(fix)
        del remaining[key]
    if remaining:
        raise Rejection(f"never found: {sorted(remaining)}")
    text = "".join(output)
    if write:
        path.write_text(text)
    return applied
