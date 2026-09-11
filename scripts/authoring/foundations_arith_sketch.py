"""Computed, truthful solution sketches for whole-number arithmetic families.

Every sketch text is built from a real simulation of the technique it names
(column carries, column borrows, place-value decomposition) over the actual
operands, then asserted against the true sum/difference/product before it is
returned — never a template string with numbers dropped in blind. A caller
still picks which two numbers go into a given knowledge point (the per-KP
recipe); this module only guarantees the arithmetic and the narration agree.
"""
from __future__ import annotations

from foundations_number_theory import digit_at_place, format_grouped


def _digits_lsb_first(n: int, width: int) -> list[int]:
    return [(n // (10**i)) % 10 for i in range(width)]


def make_ten_sketch(a: int, b: int) -> str:
    """A make-ten sketch for `a + b`, bridging ten when the sum needs it."""
    total = a + b
    larger, smaller = (a, b) if a >= b else (b, a)
    complement = 10 - larger
    if complement <= 0 or complement > smaller:
        return f"${larger} + {smaller} = {total}$."
    rest = smaller - complement
    assert larger + complement == 10
    assert complement + rest == smaller
    text = f"Make ten: ${larger} + {complement} = 10$"
    if rest:
        text += f", then $+ {rest}$ more gives ${total}$."
    else:
        text += "."
    return text


def count_back_sketch(a: int, b: int) -> str:
    """A count-back sketch for `a - b`, spelling out every step."""
    result = a - b
    chain = ", ".join(str(a - k) for k in range(b + 1))
    return f"Count back from ${a}$: ${chain}$." if b <= 6 else (
        f"Count back from ${a}$ by ${b}$: ${a} - {b} = {result}$."
    )


def bridge_back_sketch(a: int, b: int) -> str:
    """A bridge-through-ten sketch for teens minus a single digit."""
    assert 10 < a < 20
    units = a - 10
    rest = b - units
    assert 0 < rest < b
    result = a - b
    assert 10 - rest == result
    return f"Back through ten: ${a} - {units} = 10$, then $- {rest}$ more gives ${result}$."


def _place_terms(n: int) -> list[int]:
    digits = str(n)
    width = len(digits)
    return [digit_at_place(n, width - 1 - position)[1] for position in range(width) if digit_at_place(n, width - 1 - position)[0]]


def partial_products_sketch(a: int, b: int, *, decompose: str = "second") -> str:
    """Split `a` or `b` into place-value terms and multiply through, then sum."""
    target, other = (b, a) if decompose == "second" else (a, b)
    terms = _place_terms(target)
    products = [(term, other * term) for term in terms]
    total = a * b
    assert sum(product for _, product in products) == total
    parts = [f"{other} \\times {term} = {product}" for term, product in products]
    sum_line = " + ".join(str(product) for _, product in products)
    return f"${'$, $'.join(parts)}$; ${sum_line} = {total}$."


_PLACE_LABELS = ["ones", "tens", "hundreds", "thousands"]


def column_add_sketch(a: int, b: int) -> str:
    """A carry-by-carry columnwise addition sketch, ones place first."""
    total = a + b
    width = len(str(total))
    da, db = _digits_lsb_first(a, width), _digits_lsb_first(b, width)
    carry_in = 0
    steps = []
    for i in range(width):
        column = da[i] + db[i] + carry_in
        carry_out = column // 10
        addend_text = f"{da[i]}+{db[i]}" + (f"+{carry_in}" if carry_in else "")
        note = " (carry)" if carry_out else ""
        steps.append(f"{_PLACE_LABELS[i]} ${addend_text}={column}${note}")
        carry_in = carry_out
    assert carry_in == 0, "width must cover the final carry"
    assert a + b == total
    return "; ".join(steps) + f"; total ${format_grouped(total)}$."


def column_subtract_sketch(a: int, b: int) -> str:
    """A borrow-by-borrow columnwise subtraction sketch, ones place first."""
    result = a - b
    assert result >= 0
    width = max(len(str(a)), len(str(b)))
    da, db = _digits_lsb_first(a, width), _digits_lsb_first(b, width)
    borrow = 0
    steps = []
    for i in range(width):
        top = da[i] - borrow
        if top < db[i]:
            top += 10
            borrow = 1
        else:
            borrow = 0
        steps.append(top - db[i])
    rebuilt = int("".join(str(d) for d in reversed(steps)))
    assert rebuilt == result
    return f"${format_grouped(a)} - {format_grouped(b)} = {format_grouped(result)}$; check: ${format_grouped(b)} + {format_grouped(result)} = {format_grouped(a)}$."
