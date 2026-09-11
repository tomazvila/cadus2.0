"""Independently computed whole-number facts for `00-arithmetic-core` recipes.

Every function here is a thin, exact computation over Python integers (no
floats, no hardcoded fact tables) so a recipe call site can never encode a
wrong answer: the value is derived the same way a learner would derive it,
not typed in by hand. Callers still choose the concrete numbers per knowledge
point, matching that knowledge point's own authored `constraints` text — this
module only guarantees the arithmetic on top of whatever numbers a caller
picks.
"""
from __future__ import annotations

import math


def is_prime(n: int) -> bool:
    if n < 2:
        return False
    for d in range(2, math.isqrt(n) + 1):
        if n % d == 0:
            return False
    return True


def prime_factors(n: int) -> list[int]:
    """The prime factorization of `n >= 2`, ascending, with repeats."""
    if n < 2:
        raise ValueError(f"{n} has no prime factorization")
    factors = []
    remaining = n
    divisor = 2
    while divisor * divisor <= remaining:
        while remaining % divisor == 0:
            factors.append(divisor)
            remaining //= divisor
        divisor += 1
    if remaining > 1:
        factors.append(remaining)
    return factors


def factors_of(n: int) -> list[int]:
    """Every positive factor of `n`, ascending."""
    return sorted(d for d in range(1, n + 1) if n % d == 0)


def multiples_of(n: int, count: int) -> list[int]:
    """The first `count` positive multiples of `n`."""
    return [n * k for k in range(1, count + 1)]


def gcf(a: int, b: int) -> int:
    return math.gcd(a, b)


def lcm(*values: int) -> int:
    result = 1
    for value in values:
        result = result * value // math.gcd(result, value)
    return result


_PLACE_NAMES = ["ones", "tens", "hundreds", "thousands", "ten thousands", "hundred thousands"]


def digit_at_place(n: int, place_index: int) -> tuple[int, int]:
    """The `(digit, place_value)` at `place_index` (0 = ones, 1 = tens, ...)."""
    digit = (n // (10**place_index)) % 10
    return digit, digit * (10**place_index)


def place_name(place_index: int) -> str:
    return _PLACE_NAMES[place_index]


def format_grouped(n: int) -> str:
    """Render `n` in this curriculum's `{,}`-grouped thousands convention."""
    text = f"{n:,}"
    return text.replace(",", "{,}") if n >= 1000 else text


def expanded_form(n: int) -> str:
    """`n` as a sum of nonzero place values, largest place first (e.g. `4000 + 500 + 6`)."""
    digits = str(n)
    width = len(digits)
    terms = [
        digit_at_place(n, width - 1 - position)[1]
        for position in range(width)
    ]
    nonzero = [str(term) for term in terms if term != 0]
    return " + ".join(nonzero)


def round_to(n: int, place_index: int) -> int:
    """Round `n` to the place at `place_index`, half rounding up (the curriculum's own rule)."""
    scale = 10**place_index
    digit_below = (n // (scale // 10)) % 10 if place_index > 0 else 0
    base = (n // scale) * scale
    return base + scale if digit_below >= 5 else base


def divisible(n: int, d: int) -> bool:
    return n % d == 0
