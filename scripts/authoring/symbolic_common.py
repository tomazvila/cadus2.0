"""Shared exact arithmetic and formatting for the content-symbolic-a lane.

Every helper here is pure Python `fractions.Fraction` arithmetic (no float,
no external solver), so a generated exemplar's answer is computed the same
way a human would solve it by hand, and the production answer grammar
(`crates/core/src/answer`) independently checks the result when the curated
curriculum test runs against the real `Curriculum`/`ReadinessIndex`.
"""
from __future__ import annotations

import json
from fractions import Fraction as F

from foundations_curriculum_patch import NewExemplar


def frac_answer(value: F) -> str:
    """A bare `p`, `-p`, or `p/q` answer token (q > 0), matching this
    curriculum's existing answer style (e.g. `7/2`, `-1/3`)."""
    value = F(value)
    if value.denominator == 1:
        return str(value.numerator)
    return f"{value.numerator}/{value.denominator}"


def tex_frac(value: F) -> str:
    """A LaTeX `\\frac{p}{q}` (or a bare integer) for use inside `$...$`."""
    value = F(value)
    if value.denominator == 1:
        return str(value.numerator)
    sign = "-" if value.numerator < 0 else ""
    return f"{sign}\\frac{{{abs(value.numerator)}}}{{{value.denominator}}}"


def signed(n: F) -> str:
    """`+ n` or `- |n|`, for splicing a term into a running sum."""
    n = F(n)
    return f"- {frac_answer(-n)}" if n < 0 else f"+ {frac_answer(n)}"


def solve_linear(a: F, b: F, c: F) -> F:
    """The `x` solving `a*x + b = c`; `a` must be nonzero."""
    a, b, c = F(a), F(b), F(c)
    if a == 0:
        raise ValueError("a linear equation needs a nonzero leading coefficient")
    return (c - b) / a


def solve_2x2(a1: F, b1: F, c1: F, a2: F, b2: F, c2: F) -> tuple[F, F]:
    """`(x, y)` solving `a1 x + b1 y = c1` and `a2 x + b2 y = c2`, by Cramer's rule."""
    a1, b1, c1, a2, b2, c2 = (F(v) for v in (a1, b1, c1, a2, b2, c2))
    det = a1 * b2 - a2 * b1
    if det == 0:
        raise ValueError("the system is singular")
    x = (c1 * b2 - c2 * b1) / det
    y = (a1 * c2 - a2 * c1) / det
    return x, y


def slope(x1: F, y1: F, x2: F, y2: F) -> F:
    """The slope through two points; the run must be nonzero."""
    x1, y1, x2, y2 = F(x1), F(y1), F(x2), F(y2)
    if x2 == x1:
        raise ValueError("a vertical pair of points has no slope")
    return (y2 - y1) / (x2 - x1)


def label_contract(*groups: str | list[str]) -> str:
    """`{"kind":"label","options":[[...],...]}`; each group is one option's aliases."""
    options = [[g] if isinstance(g, str) else list(g) for g in groups]
    return json.dumps({"kind": "label", "options": options}, separators=(",", ":"))


EXACT = '{"kind":"exact"}'


def unit_contract(quantity: str, unit: str) -> str:
    return json.dumps(
        {"kind": "unit", "quantity": quantity, "unit": unit}, separators=(",", ":")
    )


def coordinates_contract(arity: int) -> str:
    return json.dumps({"kind": "coordinates", "arity": arity}, separators=(",", ":"))


def mk(problem: str, answer: str, sketch: str, *, contract: str | None = None) -> NewExemplar:
    """One [`NewExemplar`], `with_contract` always `False` (callers pass `contract`)."""
    return NewExemplar(problem, answer, sketch, False, contract=contract)
