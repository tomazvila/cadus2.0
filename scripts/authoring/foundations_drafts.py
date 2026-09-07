"""Generate review-only `teach` and `hint_ladder` drafts for numeric content.

Every review draft this module writes is built from one knowledge point's own
authored exemplars: [`foundations_compute.same_shape_new_operands`] redraws
the operands of one authored expression, deterministically (seeded on the
serving key), and this module's evaluator (not a guess) checks the result.
The generated worked example always lands on a value none of the knowledge
point's own exemplars serve, so it never hands the learner a served answer.
No concept sentence or hint rung below names a number. Coarse operator-family
classification is insufficient evidence for curriculum mutation, so the
solution-sketch and held-out-exemplar helpers fail closed until a KP-specific
recipe has been reviewed.
"""
from __future__ import annotations

import re
from dataclasses import dataclass
from fractions import Fraction
from random import Random
from typing import Optional

import foundations_compute as fc
import foundations_family as family_rules

# Preserve the long-standing private test seam while keeping the implementation
# in the shared family-rules module.
_IMPROPER_MIXED = family_rules._IMPROPER_MIXED
_BARE_FRACTION = family_rules._BARE_FRACTION
_TRIVIAL_FACTOR = family_rules._TRIVIAL_FACTOR
_within_kp_family = family_rules.within_kp_family
_operand_ceiling = family_rules.operand_ceiling

FAMILIES = (
    "absolute_value",
    "exponent",
    "fraction_mul_div",
    "fraction_add_sub",
    "fraction_reduce",
    "decimal_mul_div",
    "decimal_add_sub",
    "integer_mul_div",
    "integer_add_sub",
)

_EXPLICIT_MULDIV = ("\\times", "\\div", "\\cdot")


def classify_family(expr: str) -> str:
    """The coarse operator family of one authored expression, by inspection.

    The classification only chooses WORDING (the concept sentence and the
    hint ladder); [`foundations_compute.evaluate`] alone decides correctness,
    so a misclassified family costs a slightly generic hint, never a wrong
    answer.
    """
    has_abs = "|" in expr
    has_pow = "^" in expr
    has_frac = "\\frac" in expr or "\\dfrac" in expr or bool(fc._MIXED.search(expr))
    has_decimal = bool(fc._DECIMAL.search(expr))
    has_explicit_muldiv = any(token in expr for token in _EXPLICIT_MULDIV)
    stripped = expr[1:] if expr.startswith("-") else expr
    has_addsub = bool(re.search(r"[+\-]", stripped))

    if has_abs:
        return "absolute_value"
    if has_pow:
        return "exponent"
    if has_frac:
        if has_explicit_muldiv:
            return "fraction_mul_div"
        if has_addsub:
            return "fraction_add_sub"
        return "fraction_reduce"
    if has_decimal:
        return "decimal_mul_div" if has_explicit_muldiv else "decimal_add_sub"
    if has_explicit_muldiv or re.search(r"\)\s*\(|\d\s*\(", stripped):
        return "integer_mul_div"
    return "integer_add_sub"


TEACH = {
    "absolute_value": (
        "Absolute value gives a number's distance from zero, which is never negative; work out "
        "what is inside each pair of bars first, then apply whatever operation stands outside "
        "the bars.",
        [
            "Evaluate what is inside each pair of bars, then replace each bar pair with that distance.",
            "Carry out whatever operation remains outside the bars.",
        ],
    ),
    "exponent": (
        "An exponent tells you how many times to use the base as a factor, or, for a negative "
        "exponent, how many times to divide by it; simplify the power before combining it with "
        "anything else.",
        [
            "Apply the exponent to its base first, before anything else in the expression.",
            "Combine that result with the rest of the expression, in order.",
        ],
    ),
    "fraction_reduce": (
        "A fraction is in lowest terms once its numerator and its denominator share no common "
        "factor larger than one; divide both by their greatest common factor.",
        [
            "Find the greatest common factor of the numerator and the denominator.",
            "Divide both the numerator and the denominator by that factor.",
        ],
    ),
    "fraction_add_sub": (
        "Fractions add or subtract only once they share a common denominator; rewrite each "
        "fraction over that denominator, then combine the numerators and keep the denominator.",
        [
            "Rewrite every fraction over one common denominator.",
            "Add or subtract the numerators over that shared denominator, then simplify.",
        ],
    ),
    "fraction_mul_div": (
        "Multiplying fractions multiplies numerator with numerator and denominator with "
        "denominator; dividing by a fraction multiplies by its reciprocal instead.",
        [
            "If a step divides by a fraction, rewrite it as multiplying by that fraction's reciprocal.",
            "Multiply numerators together and denominators together, then simplify the result.",
        ],
    ),
    "decimal_add_sub": (
        "Decimals add and subtract like whole numbers once the decimal points of every value "
        "line up in the same column.",
        [
            "Line up the decimal points of every value in the expression.",
            "Add or subtract column by column, keeping the decimal point fixed in place.",
        ],
    ),
    "decimal_mul_div": (
        "Multiplying or dividing decimals works like whole-number arithmetic once you shift the "
        "decimal point; shift every value by the same power of ten, compute, then shift back.",
        [
            "Shift the decimal point of each value the same number of places to reach whole numbers.",
            "Multiply or divide the whole numbers, then shift the decimal point back that many places.",
        ],
    ),
    "integer_mul_div": (
        "Multiplying or dividing signed numbers: find the size as if every sign were positive, "
        "then fix the sign afterward — like signs give a positive result, unlike signs give a "
        "negative one.",
        [
            "Multiply or divide the numbers as if every sign were positive.",
            "Count the negative values: an even count gives a positive result, an odd count gives a negative one.",
        ],
    ),
    "integer_add_sub": (
        "Adding or subtracting signed numbers: rewrite any subtraction as adding the opposite "
        "value, then combine same-signed values by adding their sizes, and combine "
        "opposite-signed values by taking the difference of their sizes with the sign of the "
        "larger one.",
        [
            "Rewrite every subtraction in the expression as adding the opposite value.",
            "Combine the values in order, tracking the running size and sign.",
        ],
    ),
}

HINTS = {
    "absolute_value": [
        "What does the distance from zero look like for the value inside each pair of bars?",
        "Once each bar pair becomes that distance, what expression is left?",
        "What do you get when you work out what remains?",
    ],
    "exponent": [
        "What does the exponent tell you to do with its base?",
        "Have you applied that power before combining it with the rest of the expression?",
        "What is left once the power is worked out and combined with everything else?",
    ],
    "fraction_reduce": [
        "What number divides evenly into both the numerator and the denominator?",
        "Is that the largest such number, or could a bigger one divide both?",
        "Once you divide both parts by that number, can the result be reduced any further?",
    ],
    "fraction_add_sub": [
        "Do the fractions already share the same denominator?",
        "If not, what common denominator could every fraction share?",
        "Once the denominators match, what do you do with the numerators?",
    ],
    "fraction_mul_div": [
        "Is the step a multiplication or a division of fractions?",
        "If it divides by a fraction, what happens when you flip that fraction and multiply instead?",
        "What do you get when you multiply straight across, top with top and bottom with bottom?",
    ],
    "decimal_add_sub": [
        "Are the decimal points of every value lined up in the same column?",
        "Once they line up, how do you combine the digits in each column?",
        "Where does the decimal point belong in the final result?",
    ],
    "decimal_mul_div": [
        "Could you work this out with whole numbers instead, and place the decimal point afterward?",
        "How many places in total did you shift the decimal points of the values involved?",
        "Where does the decimal point land once you shift it back that many places?",
    ],
    "integer_mul_div": [
        "What size would the result have if every value were positive instead?",
        "How many of the values are negative?",
        "Does that count of negative values make the final result positive or negative?",
    ],
    "integer_add_sub": [
        "Could you rewrite any subtraction here as adding the opposite value instead?",
        "Which values share the same sign, and which do not?",
        "What happens when you combine the sizes of same-signed values, and take a difference for opposite-signed ones?",
    ],
}

assert set(TEACH) == set(FAMILIES)
assert set(HINTS) == set(FAMILIES)
for _rungs in HINTS.values():
    assert not any(re.search(r"\d", rung) for rung in _rungs)


@dataclass(frozen=True)
class Candidate:
    """One knowledge point ready for a generated teach page and hint ladder."""

    kp_key: str
    topic_id: str
    kp_id: str
    verb: str
    base_expr: str
    candidate_expr: str
    answer: Fraction
    family: str


def _served_values(kp: dict) -> frozenset[Fraction]:
    values = set()
    for exemplar in kp["exemplars"]:
        try:
            values.add(fc.parse_answer_text(exemplar["answer"]))
        except (ValueError, ZeroDivisionError):
            continue
    return frozenset(values)


def pure_numeric_exemplars(kp: dict) -> Optional[list[re.Match]]:
    """Every exemplar's `(verb, expr)` match, or `None` if any exemplar disqualifies the KP.

    A KP qualifies only when EVERY exemplar is a `Compute $expr$.`-shaped
    pure-numeric problem with a parseable authored answer — the same
    all-or-nothing rule [`classify_kp`] and the solution-sketch generator
    both need, kept in one place.
    """
    matches = []
    for exemplar in kp["exemplars"]:
        match = fc.match_problem(exemplar["problem"])
        if not match or not fc.is_pure_numeric(match.group(2)):
            return None
        try:
            fc.parse_answer_text(exemplar["answer"])
        except (ValueError, ZeroDivisionError):
            return None
        matches.append(match)
    return matches or None


#: A family this module declines to redraw operands for at all.
#:
#: An exponent's base and its exponent are NOT interchangeable operands: an
#: audit caught a "Squares of 1 through 15" knowledge point (a fixed
#: exponent of 2, base varies) receive a generated `6^3` — a cube, not a
#: square — and a rational-exponent knowledge point receive a degenerate
#: `7^{5/5}` (an exponent of exactly 1, which exercises no root at all).
#: [`same_shape_new_operands`] treats every bare integer as an
#: interchangeable operand and has no notion of "this one is the fixed
#: exponent" or "this one must not reduce to a trivial power"; until a
#: family-aware generator can see that distinction, the exponent family is
#: out of scope for BOTH the teach candidate and the held-out exemplar.
EXCLUDED_FROM_GENERATION = frozenset({"exponent"})


def classify_kp(topic: dict, kp: dict, rng: Random) -> Optional[Candidate]:
    """The [`Candidate`] of one knowledge point, or `None` if it is out of scope.

    Every exemplar must be a `Compute $expr$.`-shaped pure-numeric problem
    with a parseable authored answer; the new expression is built from the
    LAST such exemplar (the one closest to the knowledge point's ceiling of
    difficulty), reusing its own family and constraints.
    """
    matches = pure_numeric_exemplars(kp)
    if not matches:
        return None
    verb, base_expr = matches[-1].group(1), matches[-1].group(2)
    family = classify_family(base_expr)
    if family in EXCLUDED_FROM_GENERATION:
        return None
    served = _served_values(kp)
    try:
        candidate_expr, value = fc.same_shape_new_operands(
            base_expr,
            rng,
            forbid_zero_result=True,
            forbid_values=served,
            extra_ok=family_rules.within_kp_family(served, family),
            operand_ceiling=family_rules.operand_ceiling(matches),
        )
    except fc.NotArithmetic:
        return None
    kp_key = f"{topic['id']}/{kp['id']}"
    return Candidate(
        kp_key=kp_key,
        topic_id=topic["id"],
        kp_id=kp["id"],
        verb=verb,
        base_expr=base_expr,
        candidate_expr=candidate_expr,
        answer=value,
        family=family,
    )


def teach_draft(candidate: Candidate) -> dict:
    concept, steps = TEACH[candidate.family]
    problem = f"{candidate.verb} ${candidate.candidate_expr}$."
    # The final step states ONLY the bare answer, never the expression again:
    # the gate (`crates/core/src/instruction/teach.rs::check_no_other_answer`)
    # scans the whole last step for any token this knowledge point serves, and
    # an expression restated here would often carry an unrelated exemplar's
    # small operand or answer as an incidental digit.
    prefer_decimal = candidate.family.startswith("decimal")
    final = f"The result is ${fc.render_answer(candidate.answer, prefer_decimal=prefer_decimal)}$."
    return {
        "kp_id": candidate.kp_key,
        "kind": "teach",
        "arguments": {
            "concept": concept,
            "worked_example": {
                "problem": problem,
                "steps": [*steps, final],
            },
        },
    }


def solution_sketch_for(expr: str, answer_text: str) -> str:
    """A short, honest solution sketch for one AUTHORED (already-served) exemplar.

    Unlike the teach page, a solution sketch belongs to the exemplar itself:
    the exemplar's own `answer` field already serves this exact value, so
    restating it here names nothing the learner has not already been told.
    """
    return f"Evaluate the written operations and simplify the exact result. ${expr} = {answer_text}$."


def missing_solution_sketches(kp: dict) -> Optional[dict[int, str]]:
    """`{exemplar_index: sketch}` for every exemplar of `kp` that lacks one.

    Returns `None` when the knowledge point holds an exemplar outside the
    pure-numeric family (its exemplars are not all decidable by this
    module's evaluator, so this module writes no sketch for any of them).
    An empty dict means every exemplar already carries an authored sketch.
    """
    # A numeric equality proves arithmetic only. It does not prove that a
    # generic explanation teaches the KP's declared method (borrowing, long
    # division, powers of ten, reciprocals, and so on). Require an explicit
    # KP-aware recipe before mutating curriculum content.
    return None


@dataclass(frozen=True)
class NewExemplarPlan:
    """One generated exemplar, ready for `foundations_curriculum_patch.NewExemplar`."""

    problem: str
    answer: str
    solution_sketch: str
    with_contract: bool


def generate_held_out_exemplars(
    topic: dict, kp: dict, target: int = 4
) -> Optional[list[NewExemplarPlan]]:
    """New exemplars to raise `kp` to `target` decidable exemplars, or `None`.

    `None` means the knowledge point is out of scope (not pure-numeric) or a
    fresh operand draw ran out before reaching `target`. An empty list means
    `kp` already holds `target` or more. Every new exemplar's expression
    reuses the shape of the KP's OWN last authored exemplar
    (`same_shape_new_operands`), lands on a value none of the knowledge
    point's exemplars — authored OR already generated this call — serve, and
    carries its own solution sketch. Once curriculum readiness
    (`crates/core/src/readiness/facts.rs`) treats the LAST decidable
    exemplar as held out, the last of these new exemplars becomes that
    knowledge point's held-out assessment item.
    """
    # Same-shape operand redraws preserve syntax and arithmetic. They cannot
    # prove semantic constraints such as like denominators, regrouping,
    # multiplying by powers of ten, or required cancellation. A reviewed,
    # KP-specific recipe must replace this fail-closed path.
    return None


def hint_draft(candidate: Candidate) -> dict:
    return {
        "kp_id": candidate.kp_key,
        "kind": "hint_ladder",
        "arguments": {"hints": list(HINTS[candidate.family])},
    }
