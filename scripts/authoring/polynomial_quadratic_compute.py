"""Independent exact math for the polynomials-and-quadratics recipes.

Every recipe in this package computes an answer, then this module
recomputes it a different way (build-then-expand, or root-then-substitute)
so a transposed sign or slipped arithmetic step cannot reach the curriculum
unnoticed. All arithmetic is exact `fractions.Fraction`; nothing here calls
a model or guesses a value.
"""
from __future__ import annotations

from dataclasses import dataclass
from fractions import Fraction
from math import gcd, isqrt

Poly = tuple[Fraction, ...]  # coefficients low-to-high degree: (c0, c1, c2, ...)


def poly(*coeffs: int) -> Poly:
    """A polynomial from integer coefficients, constant term first."""
    return tuple(Fraction(c) for c in coeffs)


def trim(values: Poly) -> Poly:
    """Drop trailing zero coefficients, keeping at least one term."""
    out = list(values)
    while len(out) > 1 and out[-1] == 0:
        out.pop()
    return tuple(out)


def add(a: Poly, b: Poly) -> Poly:
    n = max(len(a), len(b))
    a = a + (Fraction(0),) * (n - len(a))
    b = b + (Fraction(0),) * (n - len(b))
    return trim(tuple(x + y for x, y in zip(a, b)))


def sub(a: Poly, b: Poly) -> Poly:
    return add(a, tuple(-x for x in b))


def scale(a: Poly, k: Fraction) -> Poly:
    return trim(tuple(x * k for x in a))


def mul(a: Poly, b: Poly) -> Poly:
    out = [Fraction(0)] * (len(a) + len(b) - 1)
    for i, x in enumerate(a):
        for j, y in enumerate(b):
            out[i + j] += x * y
    return trim(tuple(out))


def mul_many(*factors: Poly) -> Poly:
    result: Poly = poly(1)
    for factor in factors:
        result = mul(result, factor)
    return result


def linear(root: Fraction | int, lead: Fraction | int = 1) -> Poly:
    """The factor `lead*x - lead*root`, i.e. `lead*(x - root)`."""
    root = Fraction(root)
    lead = Fraction(lead)
    return trim((-lead * root, lead))


def divmod_poly(a: Poly, b: Poly) -> tuple[Poly, Poly]:
    """Exact polynomial long division; `b` must not be the zero polynomial."""
    remainder = list(trim(a))
    divisor = trim(b)
    degree_b = len(divisor) - 1
    lead_b = divisor[-1]
    degree_out = max(len(remainder) - 1 - degree_b, 0)
    quotient = [Fraction(0)] * (degree_out + 1)
    while len(trim(tuple(remainder))) - 1 >= degree_b and any(remainder):
        remainder = list(trim(tuple(remainder)))
        shift = len(remainder) - 1 - degree_b
        if shift < 0:
            break
        coeff = remainder[-1] / lead_b
        quotient[shift] = coeff
        term = [Fraction(0)] * shift + [c * coeff for c in divisor]
        remainder = list(sub(tuple(remainder), tuple(term)))
    return trim(tuple(quotient)), trim(tuple(remainder))


def synthetic_division(coeffs_desc: list[int], c: Fraction) -> tuple[list[Fraction], Fraction]:
    """Divide by `(x - c)`; `coeffs_desc` is highest-degree first.

    Returns `(quotient_desc, remainder)`.
    """
    row: list[Fraction] = []
    carry = Fraction(0)
    for coeff in coeffs_desc:
        carry = Fraction(coeff) + carry * c
        row.append(carry)
    return row[:-1], row[-1]


def evaluate(coeffs_desc: list[int] | list[Fraction], x: Fraction) -> Fraction:
    """Horner evaluation of a polynomial (highest-degree first) at `x`."""
    result = Fraction(0)
    for coeff in coeffs_desc:
        result = result * x + Fraction(coeff)
    return result


def gcf_ints(*values: int) -> int:
    result = 0
    for value in values:
        result = gcd(result, abs(value))
    return result


def squarefree(n: int) -> tuple[int, int]:
    """`n = q*q*d` with `d` squarefree and positive, for a positive integer `n`."""
    d, q, p = n, 1, 2
    while p * p <= d:
        while d % (p * p) == 0:
            d //= p * p
            q *= p
        p += 1
    return q, d


@dataclass(frozen=True)
class QuadraticRoots:
    """The exact roots of `a*x^2 + b*x + c = 0`, for integer `a`, `b`, `c`."""

    discriminant: int
    rational: tuple[Fraction, Fraction] | None  # both roots, when disc is a perfect square
    irrational: tuple[Fraction, Fraction, int] | None  # (p, q, d) meaning p +/- q*sqrt(d)


def quadratic_roots(a: int, b: int, c: int) -> QuadraticRoots:
    """Solve `a*x^2 + b*x + c = 0` exactly; `a` must be nonzero."""
    disc = b * b - 4 * a * c
    if disc < 0:
        return QuadraticRoots(disc, None, None)
    if disc == 0:
        root = Fraction(-b, 2 * a)
        return QuadraticRoots(disc, (root, root), None)
    root_q, root_d = squarefree(disc)
    if root_d == 1:
        s = isqrt(disc)
        return QuadraticRoots(disc, (Fraction(-b + s, 2 * a), Fraction(-b - s, 2 * a)), None)
    p = Fraction(-b, 2 * a)
    q = Fraction(root_q, 2 * a)
    return QuadraticRoots(disc, None, (p, q, root_d))


def vertex(a: int, b: int, c: int) -> tuple[Fraction, Fraction]:
    """The exact vertex `(h, k)` of `y = a*x^2 + b*x + c`."""
    h = Fraction(-b, 2 * a)
    k = evaluate([a, b, c], h)
    return h, k


def check_vertex_form(a: int, b: int, c: int, h: Fraction, k: Fraction) -> bool:
    """Whether `a*(x - h)^2 + k` expands back to `a*x^2 + b*x + c`, exactly."""
    expanded = add(scale(mul(linear(h), linear(h)), Fraction(a)), poly(k))
    return expanded == trim(poly(c, b, a))


def _num(c: Fraction) -> str:
    return str(c.numerator) if c.denominator == 1 else f"{c.numerator}/{c.denominator}"


def poly_to_answer(values: Poly) -> str:
    """Render a polynomial in the curriculum's plain descending-power style.

    E.g. `(-1, 5, 0, 1)` (meaning `-1 + 5x + x^3`) renders as `"x^3 + 5x - 1"`.
    """
    values = trim(values)
    terms: list[str] = []
    for degree in range(len(values) - 1, -1, -1):
        c = values[degree]
        if c == 0:
            continue
        magnitude = abs(c)
        if degree == 0:
            body = _num(magnitude)
        elif degree == 1:
            body = "x" if magnitude == 1 else f"{_num(magnitude)}x"
        else:
            body = f"x^{degree}" if magnitude == 1 else f"{_num(magnitude)}x^{degree}"
        terms.append(body if c > 0 else f"-{body}")
    if not terms:
        return "0"
    out = terms[0]
    for term in terms[1:]:
        out += f" - {term[1:]}" if term.startswith("-") else f" + {term}"
    return out
