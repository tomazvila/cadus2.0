"""Operand and result bounds for generated Foundations arithmetic."""
import math
import re
from fractions import Fraction
from typing import Callable

import foundations_compute as fc

_IMPROPER_MIXED = re.compile(r"(\d+)\\frac\{(\d+)\}\{(\d+)\}")
_BARE_FRACTION = re.compile(r"(?<!\d)\\frac\{(\d+)\}\{(\d+)\}")
#: A whole-number factor of exactly one beside `\times` or `\div`: `\times 1`,
#: `1 \times`, `\div 1` — multiplying or dividing by one trivializes the step.
_TRIVIAL_FACTOR = re.compile(r"\\(?:times|div)\s+1(?!\d)|(?<!\d)1\s+\\(?:times|div)")


def within_kp_family(served: frozenset[Fraction], family: str) -> Callable[[str, Fraction], bool]:
    """A same-shape candidate check bounding a new draw to the KP's own authored band.

    Rules a random operand redraw cannot see on its own: the result stays
    within the range this knowledge point's OWN exemplars already span
    (never an easier or a harder item than the author already picked); the
    result stays an exact integer when every authored exemplar of this
    knowledge point already is one (a "divides evenly" or "whole number"
    knowledge point never gains a fractional held-out item); a mixed
    number's own fractional part stays proper (numerator below denominator),
    so a redraw never turns `2\\frac{1}{2}` into a malformed `2\\frac{4}{4}`;
    no plain fraction operand equals exactly one (`\\frac{3}{3}`), which
    trivializes whatever it multiplies or divides; and, for the
    `fraction_reduce` family alone, the drawn fraction is not ALREADY in
    lowest terms — a "simplify this fraction" exercise needs something left
    to simplify; and no whole-number factor of exactly one sits beside a
    `\\times` or a `\\div` (multiplying or dividing by one is a no-op step).
    """
    lo, hi = min(served), max(served)
    integer_required = all(value.denominator == 1 for value in served)
    return lambda candidate, value: _candidate_within_family(
        candidate, value, lo, hi, integer_required, family
    )


def _candidate_within_family(
    candidate: str,
    value: Fraction,
    lo: Fraction,
    hi: Fraction,
    integer_required: bool,
    family: str,
) -> bool:
    """Apply the value, fraction-shape, and no-op bounds of one KP family."""
    if not lo <= value <= hi:
        return False
    if integer_required and value.denominator != 1:
        return False
    if not _proper_mixed_parts(candidate) or not _nontrivial_fractions(candidate, family):
        return False
    return _TRIVIAL_FACTOR.search(candidate) is None


def _proper_mixed_parts(candidate: str) -> bool:
    """Whether every mixed-number fraction has a numerator below its denominator."""
    return all(int(num) < int(den) for _whole, num, den in _IMPROPER_MIXED.findall(candidate))


def _nontrivial_fractions(candidate: str, family: str) -> bool:
    """Whether no fraction equals one and a reduction task still needs reduction."""
    fractions = _BARE_FRACTION.findall(candidate)
    if any(num == den for num, den in fractions):
        return False
    if family != "fraction_reduce":
        return True
    plain = next(iter(fractions), None)
    return plain is None or math.gcd(int(plain[0]), int(plain[1])) > 1


def operand_ceiling(matches: list[re.Match]) -> int:
    """The largest bare-integer operand size ANY of the KP's own exemplars uses.

    A generated operand never exceeds this, so a fraction's denominator (or
    any other operand) never drifts past what the knowledge point's own
    author already authored somewhere in it.
    """
    sizes = [
        abs(operand.value) for match in matches for operand in fc.integer_operands(match.group(2))
    ]
    return max(sizes, default=2)
